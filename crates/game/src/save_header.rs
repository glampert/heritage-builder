// Save metadata (docs/plans/realm-ui.md, Phase 1, Load / Save): what the Load / Save dialog and the
// home menu's Continue show of a save without loading it.
//
// - The header: a small object, `save_header`, written first in the save file, before the
//   session's fields (save_version first among them). Every field is optional, so a save without
//   one (made before it existed) or with fewer fields reads as a header of blanks, shown as "—",
//   and so does one this game can't read (a field of another type): the save still loads.
//   It isn't part of GameSession: no save-version bump, no migration, and session.checksum, which
//   hashes the session alone, doesn't see it.
// - read_head reads the version and the header from the start of the file only (HEAD_BYTES),
//   without parsing the session. A save the migration scripts rewrote has its version at the end
//   (Python appends it), so then the end of the file is read too; one with it anywhere else is
//   parsed whole, as before headers.
// - The thumbnail: a JPEG beside the save (saves/<name>.jpg), the city at the camera position with
//   the UI left out, SaveGameConfigs::thumbnail_size large. Written for every save made in game
//   (the autosave included), a frame after the save itself: request_thumbnail, then
//   poll_thumbnails each frame. Not on the web (engine::save::storage::save_thumbnail_path).
// - The time saved is the wall clock, which would make two runs of a script write different saves,
//   so repeatable runs leave it out (set_wall_clock: script mode, recordings and replays; CI
//   compares determinism.jsonl's save between runs and with its replay).
// - Time played is the session's runtime counter (GameSession::time_played_secs), seeded from the
//   header when a save loads.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use serde::{Deserialize, Serialize, de};

use common::mem::SingleThreadStatic;
use engine::{
    Engine,
    file_sys::paths::PathRef,
    log,
    platform,
    save::storage,
    screenshot::{ScreenshotFormat, ScreenshotId, ScreenshotOutput, ScreenshotRequest, ScreenshotScale},
};

use crate::{config::GameConfigs, custom_game::CustomGameRecord, realm::Realm};

// The header's key in the save file.
pub const SAVE_HEADER_KEY: &str = "save_header";

// How much of a save read_head reads: the header and the version come first, well within it.
const HEAD_BYTES: usize = 16 * 1024;

// The thumbnail's JPEG quality.
const THUMBNAIL_QUALITY: u8 = 85;

// ----------------------------------------------
// SaveHeader
// ----------------------------------------------

// What a save says about itself. Every field is optional: None for a save that didn't write it.
#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SaveHeader {
    // The campaign mission being played, by the campaign's id in campaigns.json and the mission's
    // index in it. None for a game outside a campaign.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub campaign_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mission_index: Option<usize>,

    // TODO(realm-ui): the city the mission founds, once missions name their cities (Phase 2's
    // world map data); a custom game's city name once Custom Game asks for one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city_name: Option<String>,

    // The realm the game is played in: its emblem in the Load / Save rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realm: Option<Realm>,

    // The in-game date: its month as an ordinal (year × 12 + month − 1, as sim.state reports it),
    // and as the realm writes it ("Long Thái era, year 3"), in the language of the save.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_ordinal: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_text: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub population: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub treasury: Option<i64>,

    // Real time spent in game in this city, across saves and loads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_played_secs: Option<f64>,

    // When it was saved, local time, "YYYY-MM-DD HH:MM:SS". Left out of repeatable runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved_at: Option<String>,

    // A custom game's map and the options the session doesn't hold itself (its realm is `realm`):
    // what a load needs to go on with it (custom_game::ActiveCustomGame::from_header). None in a
    // campaign mission.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_game: Option<CustomGameRecord>,
}

// What a save file holds: the header, then the session's fields.
#[derive(Serialize)]
pub struct SaveFileContents<'a, Session: Serialize> {
    pub save_header: &'a SaveHeader,
    #[serde(flatten)]
    pub session: &'a Session,
}

// ----------------------------------------------
// Reading the head of a save
// ----------------------------------------------

// A save's version and header.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct SaveHead {
    pub save_version: u32, // 0 for a save from before versions (as GameSession's default).
    pub header: Option<SaveHeader>,
}

// The version alone, for a save whose version isn't at its start or end: the whole file is parsed.
#[derive(Deserialize)]
struct SaveVersionProbe {
    #[serde(default)]
    save_version: u32,
}

pub fn read_head(save_file: PathRef) -> Result<SaveHead, String> {
    let text = storage::read_save_file_head(save_file, HEAD_BYTES)?;
    let (head, version) = parse_head(&text)?;
    if version.is_some() {
        return Ok(head);
    }
    let save_version = match version_in_tail(&storage::read_save_file_tail(save_file, HEAD_BYTES)?) {
        Some(version) => version,
        None => storage::load_save_file::<SaveVersionProbe>(save_file)?.save_version,
    };
    Ok(SaveHead { save_version, ..head })
}

// The size in cells of a save's map, from the start of the file: the tile map's `size_in_cells`,
// its first field, which comes after the header and the version. None if it isn't there.
pub fn read_map_size(save_file: PathRef) -> Option<common::Size> {
    let text = storage::read_save_file_head(save_file, HEAD_BYTES).ok()?;
    map_size_in_head(&text)
}

pub fn map_size_in_head(text: &str) -> Option<common::Size> {
    #[derive(Deserialize)]
    struct SizeJson {
        width: i32,
        height: i32,
    }
    let tile_map = text.find("\"tile_map\"")?;
    let key = "\"size_in_cells\"";
    let rest = &text[tile_map..];
    let rest = rest[rest.find(key)? + key.len()..].trim_start().strip_prefix(':')?;
    let size = serde_json::Deserializer::from_str(rest).into_iter::<SizeJson>().next()?.ok()?;
    (size.width > 0 && size.height > 0).then(|| common::Size::new(size.width, size.height))
}

// The version at the end of a save's text, where the migration scripts write it: the number after
// the last "save_version" key (no other key of a save is called that).
pub fn version_in_tail(tail: &str) -> Option<u32> {
    let key = "\"save_version\"";
    let rest = tail[tail.rfind(key)? + key.len()..].trim_start().strip_prefix(':')?.trim_start();
    let digits = rest.find(|c: char| !c.is_ascii_digit()).map_or(rest, |end| &rest[..end]);
    digits.parse().ok()
}

// The version and header at the start of a save's text, which may be cut anywhere after them, and
// the version if it was there. Reads the top-level keys in order while they're the header or the
// version, and stops at the first other one (the session's), without parsing it.
pub fn parse_head(text: &str) -> Result<(SaveHead, Option<u32>), String> {
    struct HeadVisitor<'a> {
        head: &'a mut SaveHead,
        version: &'a mut Option<u32>,
        done: &'a mut bool,
    }

    impl<'de> de::Visitor<'de> for HeadVisitor<'_> {
        type Value = ();

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a save game object")
        }

        fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
            while let Some(key) = map.next_key::<String>()? {
                match key.as_str() {
                    // Read as JSON first: a header this game can't read (a field of another
                    // type) is blank, and the save still loads.
                    SAVE_HEADER_KEY => {
                        let value = map.next_value::<serde_json::Value>()?;
                        let header = serde_json::from_value(value).unwrap_or_else(|err| {
                            log::warning!(log::channel!("session"), "Ignoring a save header this game can't read: {err}");
                            SaveHeader::default()
                        });
                        self.head.header = Some(header);
                    }
                    "save_version" => *self.version = Some(map.next_value()?),
                    _ => break,
                }
            }
            // The rest is the session's: stop here. The deserializer then fails on what's left
            // (or on the cut), which read_head ignores once this is set.
            *self.done = true;
            Err(de::Error::custom("save head read"))
        }
    }

    let mut head = SaveHead::default();
    let mut version = None;
    let mut done = false;
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let visitor = HeadVisitor { head: &mut head, version: &mut version, done: &mut done };
    let result = de::Deserializer::deserialize_map(&mut deserializer, visitor);

    match result {
        Ok(()) => {}
        Err(_) if done => {}
        Err(err) => return Err(format!("Can't read the save's header: {err}")),
    }
    head.save_version = version.unwrap_or(0);
    Ok((head, version))
}

// ----------------------------------------------
// Time saved
// ----------------------------------------------

static WALL_CLOCK: AtomicBool = AtomicBool::new(true);

// Whether saves record the time they're made. Off for runs that must write the same saves each
// time: scripts, recordings and replays (GameLoop::start).
pub fn set_wall_clock(enabled: bool) {
    WALL_CLOCK.store(enabled, Ordering::Relaxed);
}

// The local time, as a header's saved_at; None in repeatable runs.
pub fn saved_at_now() -> Option<String> {
    if !WALL_CLOCK.load(Ordering::Relaxed) {
        return None;
    }
    let now = platform::local_date_time()?;
    Some(format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", now.year, now.month, now.day, now.hour, now.minute, now.second))
}

// A saved_at's date and time: (year, month, day, hour, minute). None if it doesn't parse.
pub fn parse_saved_at(text: &str) -> Option<(i32, u32, u32, u32, u32)> {
    let (date, time) = text.split_once(' ')?;
    let mut date = date.split('-');
    let year = date.next()?.parse().ok()?;
    let month = date.next()?.parse().ok()?;
    let day = date.next()?.parse().ok()?;
    let mut time = time.split(':');
    let hour = time.next()?.parse().ok()?;
    let minute = time.next()?.parse().ok()?;
    ((1..=12).contains(&month) && (1..=31).contains(&day) && hour < 24 && minute < 60).then_some((year, month, day, hour, minute))
}

// ----------------------------------------------
// Revision
// ----------------------------------------------

static REVISION: AtomicU32 = AtomicU32::new(0);

// Goes up whenever a save, its thumbnail or its deletion changes what's in saves/, so the Load /
// Save dialog lists them again.
pub fn revision() -> u32 {
    REVISION.load(Ordering::Relaxed)
}

pub fn bump_revision() {
    REVISION.fetch_add(1, Ordering::Relaxed);
}

// ----------------------------------------------
// Thumbnails
// ----------------------------------------------

static PENDING_THUMBNAILS: SingleThreadStatic<Vec<(ScreenshotId, String)>> = SingleThreadStatic::new(Vec::new());

// Captures this frame's city, without the UI, as the save's thumbnail. Call it in game, after the
// save is written. Nothing on the web, whose saves keep no thumbnail; with no valid
// thumbnail_size, the save's old thumbnail goes (remove_stale_thumbnail).
pub fn request_thumbnail(engine: &mut Engine, configs: &GameConfigs, save_file: PathRef) {
    let Some(path) = storage::save_thumbnail_path(save_file) else {
        return;
    };
    let size = configs.save.thumbnail_size;
    if !size.is_valid() {
        remove_stale_thumbnail(save_file);
        return;
    }
    let id = engine.request_screenshot(ScreenshotRequest {
        include_ui: false,
        scale: ScreenshotScale::Cover(size),
        format: ScreenshotFormat::Jpeg(THUMBNAIL_QUALITY),
        output: ScreenshotOutput::File(path.as_str().into()),
        // Written in the frame it's captured, so the Load / Save dialog shows it after the same
        // number of frames on every run (scripts). A thumbnail is small: a few milliseconds.
        encode_in_frame: true,
    });
    PENDING_THUMBNAILS.as_mut().push((id, save_file.as_str().to_string()));
}

// Deletes the thumbnail a save had before, for a save that writes no new one (made in the home
// menu, or whose capture failed), so its preview doesn't show the city it replaced.
pub fn remove_stale_thumbnail(save_file: PathRef) {
    match storage::delete_save_thumbnail(save_file) {
        Ok(()) => bump_revision(),
        Err(err) => log::error!(log::channel!("session"), "{err}"),
    }
}

// Collects the thumbnails written since last frame. Call once per frame.
pub fn poll_thumbnails(engine: &mut Engine) {
    let pending = PENDING_THUMBNAILS.as_mut();
    if pending.is_empty() {
        return;
    }
    pending.retain(|(id, save)| {
        let Some(result) = engine.take_screenshot_result(*id) else {
            return true;
        };
        match result {
            Ok(_) => log::info!(log::channel!("session"), "Saved the thumbnail of '{save}'."),
            Err(err) => {
                log::error!(log::channel!("session"), "Failed to save the thumbnail of '{save}': {err}");
                remove_stale_thumbnail(PathRef::from_str(save));
            }
        }
        bump_revision();
        false
    });
}

// ----------------------------------------------
// Unit tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_sizes_from_the_head() {
        let pretty = "{\n  \"tile_map\": {\n    \"size_in_cells\": {\n      \"width\": 64,\n      \"height\": 96\n    },\n    \"layers\": [";
        assert_eq!(map_size_in_head(pretty), Some(common::Size::new(64, 96)));
        let compact = r#"{"save_header":{"realm":"dailong"},"save_version":7,"tile_map":{"size_in_cells":{"width":24,"height":24},"layers":["#;
        assert_eq!(map_size_in_head(compact), Some(common::Size::new(24, 24)));
        assert_eq!(map_size_in_head(r#"{"save_version":7,"tile_map":{"size_in_cells":{"width":2"#), None); // Cut off.
        assert_eq!(map_size_in_head("{}"), None);
    }

    #[test]
    fn head_of_a_save_with_a_header() {
        let header = SaveHeader {
            campaign_id: Some("dragon_legacy".into()),
            mission_index: Some(1),
            realm: Some(Realm::Kambara),
            population: Some(120),
            treasury: Some(-40),
            time_played_secs: Some(61.5),
            ..SaveHeader::default()
        };
        let session = serde_json::json!({ "save_version": 7, "tile_map": { "huge": [1, 2, 3] } });
        let text = serde_json::to_string_pretty(&SaveFileContents { save_header: &header, session: &session }).unwrap();
        assert!(text.trim_start().starts_with("{\n  \"save_header\""), "The header comes first: {text}");

        let head = parse_head(&text).unwrap();
        assert_eq!(head, (SaveHead { save_version: 7, header: Some(header.clone()) }, Some(7)));

        // Cut in the session: still read.
        let cut = &text[..text.find("huge").unwrap()];
        assert_eq!(parse_head(cut).unwrap(), (SaveHead { save_version: 7, header: Some(header) }, Some(7)));
    }

    #[test]
    fn head_of_an_old_save() {
        // No header; the version first, as GameSession writes it.
        assert_eq!(parse_head(r#"{"save_version": 6, "tile_map": {"#).unwrap(), (SaveHead { save_version: 6, header: None }, Some(6)));
        // Not at the start: before versions, or at the end, where the migration scripts put it.
        assert_eq!(parse_head(r#"{"tile_map": {"#).unwrap(), (SaveHead { save_version: 0, header: None }, None));
        assert_eq!(version_in_tail("  \"is_zooming\": false\n  },\n  \"save_version\": 7\n}\n"), Some(7));
        assert_eq!(version_in_tail("\"save_version\":12}"), Some(12));
        assert_eq!(version_in_tail("\"camera\": {}}"), None);
        // A header of fields this game doesn't know, and missing ones.
        let (head, _) = parse_head(r#"{"save_header": {"population": 5, "from_the_future": true}, "save_version": 8}"#).unwrap();
        assert_eq!(head.save_version, 8);
        assert_eq!(head.header.unwrap(), SaveHeader { population: Some(5), ..SaveHeader::default() });
        // A header of another shape: blank, the version still read.
        let (head, version) = parse_head(r#"{"save_header": {"population": -1, "realm": "elsewhere"}, "save_version": 7, "x": 1}"#).unwrap();
        assert_eq!((head.header, version), (Some(SaveHeader::default()), Some(7)));
        let (head, version) = parse_head(r#"{"save_header": [1], "save_version": 7}"#).unwrap();
        assert_eq!((head.header, version), (Some(SaveHeader::default()), Some(7)));
        // Not a save.
        assert!(parse_head("[1, 2]").is_err());
        assert!(parse_head("").is_err());
    }

    #[test]
    fn saved_at_parses() {
        assert_eq!(parse_saved_at("2026-11-04 21:14:05"), Some((2026, 11, 4, 21, 14)));
        assert_eq!(parse_saved_at("2026-13-04 21:14:05"), None);
        assert_eq!(parse_saved_at("yesterday"), None);
    }
}
