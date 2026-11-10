// Localized strings (docs/plans/realm-ui.md, Text).
//
// One file per language, assets/configs/ui/text/<language>.json: an object of categories, by
// name, each an object of keys. A key's value is its text, or its plural forms picked by a count
// (`{"one": "{count} month", "other": "{count} months"}`, the CLDR form names; `other` is
// required). Text may hold `{name}` placeholders, filled by format and format_count (`{{` and `}}`
// write braces).
//
// Every language file is loaded at boot, since the web build can read an asset only once, and
// none is ever dropped, so the text handed out is &'static. The language is switched by name.
//
// A lookup tries the current language, then English, logging the fallback once per key; a key
// missing in English too shows as the key itself, so it's caught on screen, and is logged once
// and listed by missing_keys (the text.missing command).
//
// Keys the code names: write the category as `UiTextCategory::<Name>` with the key's literal
// right after it, e.g. `text::get(UiTextCategory::Menu, "button.cancel")` or
// `UiString::key(UiTextCategory::Menu, "button.cancel")`. The key check (tests below) finds them
// that way in every crate's sources and checks en.json has them.

use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    fmt,
    str::FromStr,
    sync::{
        Mutex, OnceLock, PoisonError,
        atomic::{AtomicU32, AtomicUsize, Ordering},
    },
};

use common::hash::{self, NULL_HASH, PreHashedKeyMap, StringHash};
use serde::{
    Deserialize, Serialize,
    de::{self, DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor},
};
use serde_json::Value;
use strum::{Display, EnumCount, EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

use crate::{
    config,
    file_sys::{self, CollectFlags, paths::AssetPath},
    log,
    ui::UiSystem,
};

// ----------------------------------------------
// Public API
// ----------------------------------------------

// A key's FNV-1a hash, e.g. a TileDef's `hash` (the hash of its name).
pub type UiTextKey = StringHash;

// English: the fallback for every other language, and the file every key must be in.
pub const ENGLISH: &str = "en";

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
    Display, EnumString, EnumIter, EnumCount, IntoStaticStr,
)]
pub enum UiTextCategory {
    TileDescription, // By tile def name: the inspector's text.
    UnitDialog,      // By unit config name: what a unit says in the inspector.
    Menu,            // Dialogs, popups and their buttons.
    Hud,             // The in-game bars, palette and inspectors.
    City,            // The City book.
    Story,           // Briefings, steles and comics.
    Realm,           // Realm wording: eras, seasons, ratings, campaign subtitles.
    Messages,        // Toasts and the message log.
}

// Loads every language file and switches to `language` (English if it isn't there). Called once
// at engine init; a lookup before then loads the files itself (tests, tools).
pub fn initialize(language: &str) {
    let store = store();

    if store.languages.is_empty() {
        return; // Loading logged why.
    }

    let names: Vec<&str> = store.languages.iter().map(|language| language.name.as_str()).collect();
    log::info!(log::channel!("ui"), "Languages: {}.", names.join(", "));

    if let Err(err) = set_language(language) {
        log::error!(log::channel!("ui"), "{err} Using English.");
    }
}

// The languages loaded, by name, sorted.
pub fn languages() -> Vec<&'static str> {
    store().languages.iter().map(|language| language.name.as_str()).collect()
}

// The current language's name; English when no file loaded.
pub fn current_language() -> &'static str {
    let store = store();
    store.current().map_or(ENGLISH, |index| store.languages[index].name.as_str())
}

// Switches to a loaded language. Text drawn from a UiString follows at once; text a menu measured
// or copied when it was built follows when it's built again (see version).
pub fn set_language(language: &str) -> Result<(), String> {
    let store = store();
    let Some(index) = store.language_index(language) else {
        return Err(format!("No language '{language}' (assets/configs/ui/text/{language}.json). Languages: {}.", languages().join(", ")));
    };

    if CURRENT_LANGUAGE.swap(index, Ordering::Relaxed) != index {
        TEXT_VERSION.fetch_add(1, Ordering::Relaxed);
        log::info!(log::channel!("ui"), "Language: {language}.");
    }
    Ok(())
}

// Goes up with every language switch, for UI that keeps text it looked up.
pub fn version() -> u32 {
    TEXT_VERSION.load(Ordering::Relaxed)
}

// The key's text in the current language, else in English, else the key itself.
pub fn get<'a>(category: UiTextCategory, key: &'a str) -> &'a str {
    get_hashed(category, key, hash::fnv1a_from_str(key))
}

// The key's text with its `{name}` placeholders filled from `args`; a placeholder that isn't in
// `args` is left as it is.
pub fn format(category: UiTextCategory, key: &str, args: &[(&str, &str)]) -> String {
    fill_placeholders(get(category, key), args)
}

// Like format, with the key's plural form for `count` (in the language the text comes from), and
// `{count}` filled with it unless `args` gives it. A key without plural forms is used as it is.
pub fn format_count(category: UiTextCategory, key: &str, count: i64, args: &[(&str, &str)]) -> String {
    let text = match lookup(category, key, hash::fnv1a_from_str(key)) {
        Some((entry, language)) => plural_text(entry, language, count),
        None => key,
    };
    fill_count(text, count, args)
}

// The entry's form for `count` by the plural rule of `language`, the one it comes from (English
// for a key the current language lacks).
fn plural_text<'a>(entry: &'a UiTextEntry, language: &UiLanguage, count: i64) -> &'a str {
    entry.form(language.plural_rule.form(count))
}

// `text` with `{count}` filled with `count` unless `args` gives it, and the other placeholders from `args`.
fn fill_count(text: &str, count: i64, args: &[(&str, &str)]) -> String {
    let count_text = count.to_string();
    if args.iter().any(|(name, _)| *name == "count") {
        fill_placeholders(text, args)
    } else {
        let mut args_with_count: Vec<(&str, &str)> = Vec::with_capacity(args.len() + 1);
        args_with_count.extend_from_slice(args);
        args_with_count.push(("count", &count_text));
        fill_placeholders(text, &args_with_count)
    }
}

// Optional text by hashed key, e.g. a tile's description by its TileDef hash: the current
// language's, else English's. None if neither has it, or it's empty; that isn't a miss.
pub fn find_str(category: UiTextCategory, key: UiTextKey) -> Option<&'static str> {
    if key == NULL_HASH {
        return None;
    }

    let store = store();
    let current = store.current();
    let (entry, language) = store.find(current, category, key)?;

    // A fallback to English counts as untranslated, as in `lookup`; the key's text isn't known
    // here, so it's recorded by its hash.
    if let Some(index) = current
        && !std::ptr::eq(language, &store.languages[index])
    {
        let hashed_key = common::format_small!("#{key:x}");
        if record_miss(Some(index), category, &hashed_key) {
            let current_name = &store.languages[index].name;
            log::warning!(log::channel!("ui"), "No {category} '{hashed_key}' in language '{current_name}': using English.");
        }
    }

    let text = entry.text();
    (!text.is_empty()).then_some(text)
}

// What a lookup would give, without logging or recording a miss: the text and the language it
// comes from (None for a missing key, whose text is the key). For automation (text.get).
pub fn peek(category: UiTextCategory, key: &str, count: Option<i64>) -> (String, Option<&'static str>) {
    let store = store();
    match store.find(store.current(), category, hash::fnv1a_from_str(key)) {
        Some((entry, language)) => {
            let text = count.map_or(entry.text(), |count| plural_text(entry, language, count));
            (text.to_string(), Some(language.name.as_str()))
        }
        None => (key.to_string(), None),
    }
}

// The keys looked up and missing in English too (shown as the key), by category, since the start
// or the last reset_misses.
pub fn missing_keys() -> BTreeMap<UiTextCategory, Vec<String>> {
    let misses = MISSES.lock().unwrap_or_else(PoisonError::into_inner);
    misses.missing.iter().map(|(category, keys)| (*category, keys.iter().cloned().collect())).collect()
}

// The keys looked up that a language lacks and English has (shown in English), by language and
// category, since the start or the last reset_misses.
pub fn untranslated_keys() -> BTreeMap<String, BTreeMap<UiTextCategory, Vec<String>>> {
    let store = store();
    let misses = MISSES.lock().unwrap_or_else(PoisonError::into_inner);
    let mut result: BTreeMap<String, BTreeMap<UiTextCategory, Vec<String>>> = BTreeMap::new();
    for ((language, category), keys) in &misses.untranslated {
        let name = store.languages[*language].name.clone();
        result.entry(name).or_default().insert(*category, keys.iter().cloned().collect());
    }
    result
}

// Forgets the misses recorded so far; a key missed again is logged again.
pub fn reset_misses() {
    let mut misses = MISSES.lock().unwrap_or_else(PoisonError::into_inner);
    misses.missing.clear();
    misses.untranslated.clear();
}

pub fn draw_debug_ui(ui_sys: &UiSystem) {
    use crate::ui::DrawDebugUi;
    UiTextDebug.draw_debug_ui(ui_sys);
}

// ----------------------------------------------
// UiString
// ----------------------------------------------

// A widget's text: a key, looked up each time it's drawn so it follows a language switch, or
// literal text, e.g. a number or a name from the game's data. `&str` and `String` convert to
// literals, so a widget setter taking `impl Into<UiString>` takes them as before.
//
// Automation records the text as drawn, so in English the widget paths are the English text.
// TODO(realm-ui): a translated build should record a key's English text (or the key) as the
// path, so scenarios work in any language.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum UiString {
    Literal(String),
    Key { category: UiTextCategory, key: Cow<'static, str>, hash: UiTextKey },
}

impl UiString {
    // A key the code names. Write the category as UiTextCategory::<Name> (see the top of the file).
    pub const fn key(category: UiTextCategory, key: &'static str) -> Self {
        Self::Key { category, key: Cow::Borrowed(key), hash: hash::fnv1a_from_str(key) }
    }

    pub fn literal(text: impl Into<String>) -> Self {
        Self::Literal(text.into())
    }

    // The text to draw: the key's in the current language (see get), or the literal.
    pub fn resolve(&self) -> &str {
        match self {
            Self::Literal(text) => text,
            Self::Key { category, key, hash } => get_hashed(*category, key, *hash),
        }
    }

    // What stays the same across language switches, e.g. for a widget's egui id: the key, or the
    // literal text.
    pub fn id_source(&self) -> &str {
        match self {
            Self::Literal(text) => text,
            Self::Key { key, .. } => key,
        }
    }

    #[cfg(test)]
    pub fn is_key(&self) -> bool {
        matches!(self, Self::Key { .. })
    }

    // True for empty literal text; a key is never empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Literal(text) if text.is_empty())
    }

    // Empty literal text, keeping the buffer.
    pub fn clear(&mut self) {
        match self {
            Self::Literal(text) => text.clear(),
            Self::Key { .. } => *self = Self::default(),
        }
    }

    // Literal text, reusing the buffer, e.g. for a stat updated every frame.
    pub fn set_literal(&mut self, text: &str) {
        match self {
            Self::Literal(buffer) => {
                buffer.clear();
                buffer.push_str(text);
            }
            Self::Key { .. } => *self = Self::Literal(text.to_string()),
        }
    }
}

impl Default for UiString {
    fn default() -> Self {
        Self::Literal(String::new())
    }
}

impl fmt::Display for UiString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.resolve())
    }
}

impl From<&str> for UiString {
    fn from(text: &str) -> Self {
        Self::Literal(text.to_string())
    }
}

impl From<&String> for UiString {
    fn from(text: &String) -> Self {
        Self::Literal(text.clone())
    }
}

impl From<String> for UiString {
    fn from(text: String) -> Self {
        Self::Literal(text)
    }
}

impl From<Cow<'_, str>> for UiString {
    fn from(text: Cow<'_, str>) -> Self {
        Self::Literal(text.into_owned())
    }
}

impl From<&UiString> for UiString {
    fn from(text: &UiString) -> Self {
        text.clone()
    }
}

// ----------------------------------------------
// Placeholders
// ----------------------------------------------

// `{name}` from `args`; `{{` and `}}` are braces. Unknown or unclosed placeholders stay as written.
// What format does to a key's text, for text from elsewhere (e.g. peek's).
pub fn fill_placeholders(text: &str, args: &[(&str, &str)]) -> String {
    let mut result = String::with_capacity(text.len() + 16);
    let mut rest = text;

    while let Some(pos) = rest.find(['{', '}']) {
        result.push_str(&rest[..pos]);
        let tail = &rest[pos..];

        if tail.starts_with("{{") || tail.starts_with("}}") {
            result.push_str(&tail[..1]);
            rest = &tail[2..];
        } else if let Some(name) = tail.strip_prefix('{').and_then(|after| after.split_once('}')).map(|(name, _)| name)
            && is_placeholder_name(name)
            && let Some((_, value)) = args.iter().find(|(arg, _)| *arg == name)
        {
            result.push_str(value);
            rest = &tail[name.len() + 2..];
        } else {
            result.push_str(&tail[..1]);
            rest = &tail[1..];
        }
    }

    result.push_str(rest);
    result
}

fn is_placeholder_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// The placeholders a text names, e.g. ["count", "months"].
fn placeholder_names(text: &str) -> BTreeSet<&str> {
    let mut names = BTreeSet::new();
    let mut rest = text;
    while let Some(pos) = rest.find('{') {
        let tail = &rest[pos..];
        if let Some(after) = tail.strip_prefix("{{") {
            rest = after;
        } else if let Some((name, _)) = tail[1..].split_once('}')
            && is_placeholder_name(name)
        {
            names.insert(name);
            rest = &tail[name.len() + 2..];
        } else {
            rest = &tail[1..];
        }
    }
    names
}

// ----------------------------------------------
// Plural forms
// ----------------------------------------------

// The CLDR plural categories. A plural entry gives the forms its language uses.
#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumIter, EnumString, EnumCount, Display, IntoStaticStr)]
#[strum(serialize_all = "lowercase")]
pub enum PluralForm {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

// Which form a count takes in a language. Add a language's rule from the CLDR plural rules
// (https://cldr.unicode.org): e.g. Russian's one/few/many would be a new variant.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum PluralRule {
    OneOther,  // English and most European languages: 1 is `one`, anything else `other`.
    OtherOnly, // Languages without plural inflection, e.g. Vietnamese: always `other`.
}

impl PluralRule {
    fn for_language(language: &str) -> Self {
        match language {
            "vi" | "ja" | "ko" | "zh" | "th" | "id" | "ms" | "km" | "lo" => Self::OtherOnly,
            _ => Self::OneOther,
        }
    }

    fn form(self, count: i64) -> PluralForm {
        match self {
            Self::OneOther if count.unsigned_abs() == 1 => PluralForm::One,
            Self::OneOther | Self::OtherOnly => PluralForm::Other,
        }
    }
}

// ----------------------------------------------
// UiTextEntry
// ----------------------------------------------

// A key's value: its text, or its plural forms (`Other` always given).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum UiTextEntry {
    Plain(String),
    Plural(Box<[Option<String>; PluralForm::COUNT]>),
}

impl UiTextEntry {
    fn from_value(value: Value) -> Result<Self, String> {
        match value {
            Value::String(text) => Ok(Self::Plain(text)),
            Value::Object(forms) => {
                let mut plural: [Option<String>; PluralForm::COUNT] = Default::default();
                for (form_name, text) in forms {
                    let Ok(form) = PluralForm::from_str(&form_name) else {
                        let names: Vec<String> = PluralForm::iter().map(|form| form.to_string()).collect();
                        return Err(format!("unknown plural form '{form_name}' (forms: {})", names.join(", ")));
                    };
                    let Value::String(text) = text else {
                        return Err(format!("plural form '{form_name}' isn't text"));
                    };
                    plural[form as usize] = Some(text);
                }
                if plural[PluralForm::Other as usize].is_none() {
                    return Err("plural forms need 'other'".to_string());
                }
                Ok(Self::Plural(Box::new(plural)))
            }
            _ => Err("a key's value is its text, or an object of plural forms".to_string()),
        }
    }

    // The text, or the `other` form.
    pub(crate) fn text(&self) -> &str {
        self.form(PluralForm::Other)
    }

    // The given form, else `other`.
    pub(crate) fn form(&self, form: PluralForm) -> &str {
        match self {
            Self::Plain(text) => text,
            Self::Plural(forms) => {
                forms[form as usize].as_deref().or(forms[PluralForm::Other as usize].as_deref()).unwrap_or_default()
            }
        }
    }

    // Each form given, with its name ("" for plain text).
    pub(crate) fn forms(&self) -> Vec<(&'static str, &str)> {
        match self {
            Self::Plain(text) => vec![("", text.as_str())],
            Self::Plural(forms) => PluralForm::iter()
                .filter_map(|form| forms[form as usize].as_deref().map(|text| (form.into(), text)))
                .collect(),
        }
    }
}

// ----------------------------------------------
// UiLanguage
// ----------------------------------------------

// One category's keys and their text, sorted by key.
#[derive(Default)]
pub(crate) struct UiTextCategoryStrings {
    pub(crate) entries: Vec<(String, UiTextEntry)>,
    lookup: PreHashedKeyMap<UiTextKey, usize>, // Key hash => entries[index]
}

// One language file, loaded.
pub(crate) struct UiLanguage {
    pub(crate) name: String,
    plural_rule: PluralRule,
    categories: [UiTextCategoryStrings; UiTextCategory::COUNT],
}

impl UiLanguage {
    // Parses a language file. A file that isn't a JSON object is an error; a bad category or entry
    // is left out, and a key given twice keeps its last value, each listed in the problems returned.
    fn parse(name: &str, json: &str) -> Result<(Self, Vec<String>), String> {
        let value: Value = serde_json::from_str(json).map_err(|err| format!("not valid JSON: {err}"))?;
        let Value::Object(categories) = value else {
            return Err("not a JSON object of categories".to_string());
        };

        let mut language = Self {
            name: name.to_string(),
            plural_rule: PluralRule::for_language(name),
            categories: Default::default(),
        };
        let mut problems: Vec<String> =
            duplicate_keys(json).into_iter().map(|path| format!("{path}: given twice; the last one is used.")).collect();

        for (category_name, strings) in categories {
            let Ok(category) = UiTextCategory::from_str(&category_name) else {
                let names: Vec<&str> = UiTextCategory::iter().map(<&str>::from).collect();
                problems.push(format!("Unknown category '{category_name}' (categories: {}).", names.join(", ")));
                continue;
            };
            let Value::Object(strings) = strings else {
                problems.push(format!("{category}: not an object of keys."));
                continue;
            };

            let category_strings = &mut language.categories[category as usize];
            for (key, value) in strings {
                if key.is_empty() {
                    problems.push(format!("{category}: an empty key."));
                    continue;
                }
                match UiTextEntry::from_value(value) {
                    Ok(entry) => category_strings.entries.push((key, entry)),
                    Err(err) => problems.push(format!("{category}/{key}: {err}.")),
                }
            }

            category_strings.entries.sort_by(|(a, _), (b, _)| a.cmp(b));
            for (index, (key, _)) in category_strings.entries.iter().enumerate() {
                if category_strings.lookup.insert(hash::fnv1a_from_str(key), index).is_some() {
                    problems.push(format!("{category}/{key}: its hash collides with another key's."));
                }
            }
        }

        Ok((language, problems))
    }

    fn entry(&self, category: UiTextCategory, key: UiTextKey) -> Option<&UiTextEntry> {
        let strings = &self.categories[category as usize];
        strings.lookup.get(&key).map(|index| &strings.entries[*index].1)
    }

    pub(crate) fn strings(&self, category: UiTextCategory) -> &UiTextCategoryStrings {
        &self.categories[category as usize]
    }

    // Every (category, key) it has.
    fn keys(&self) -> BTreeSet<(UiTextCategory, &str)> {
        UiTextCategory::iter()
            .flat_map(|category| self.categories[category as usize].entries.iter().map(move |(key, _)| (category, key.as_str())))
            .collect()
    }
}

// The paths of the keys a JSON text gives twice in an object, e.g. "Menu/button.yes", which
// serde_json's Value drops silently (keeping the last).
fn duplicate_keys(json: &str) -> Vec<String> {
    let mut duplicates = Vec::new();
    let mut deserializer = serde_json::Deserializer::from_str(json);
    // A JSON error is the Value parse's to report.
    let _ = DuplicateKeys { path: String::new(), duplicates: &mut duplicates }.deserialize(&mut deserializer);
    duplicates
}

// Walks a JSON value, listing the keys repeated in each object.
struct DuplicateKeys<'a> {
    path: String,
    duplicates: &'a mut Vec<String>,
}

impl<'de> DeserializeSeed<'de> for DuplicateKeys<'_> {
    type Value = ();

    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for DuplicateKeys<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E>(self, _: bool) -> Result<(), E> { Ok(()) }
    fn visit_i64<E>(self, _: i64) -> Result<(), E> { Ok(()) }
    fn visit_u64<E>(self, _: u64) -> Result<(), E> { Ok(()) }
    fn visit_f64<E>(self, _: f64) -> Result<(), E> { Ok(()) }
    fn visit_str<E>(self, _: &str) -> Result<(), E> { Ok(()) }
    fn visit_unit<E>(self) -> Result<(), E> { Ok(()) }

    // No array is valid in a language file, so their contents aren't checked.
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut keys = BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            let path = if self.path.is_empty() { key.clone() } else { format!("{}/{key}", self.path) };
            if !keys.insert(key) {
                self.duplicates.push(path.clone());
            }
            map.next_value_seed(DuplicateKeys { path, duplicates: &mut *self.duplicates })?;
        }
        Ok(())
    }
}

// ----------------------------------------------
// UiTextStore
// ----------------------------------------------

// Every language loaded, sorted by name. Never changes once loaded.
pub(crate) struct UiTextStore {
    pub(crate) languages: Vec<UiLanguage>,
    english: Option<usize>,
}

impl UiTextStore {
    fn new(mut languages: Vec<UiLanguage>) -> Self {
        languages.sort_by(|a, b| a.name.cmp(&b.name));
        let english = languages.iter().position(|language| language.name == ENGLISH);
        Self { languages, english }
    }

    // Every assets/configs/ui/text/*.json. A file that can't be read or parsed is an error, and
    // its language isn't available; without en.json every key shows as itself.
    fn load() -> Self {
        let dir = text_dir();
        let files = file_sys::collect_files(&dir, CollectFlags::FilenamesOnly | CollectFlags::ErrorIfPathDoesNotExist, Some("json"))
            .unwrap_or_else(|err| {
                log::error!(log::channel!("ui"), "Can't list the language files in {dir}: {err}");
                Vec::new()
            });

        let mut languages = Vec::new();
        for file in files {
            let Some(name) = file.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let path = dir.join(file.to_string_lossy());

            let json = match file_sys::load_string(&path) {
                Ok(json) => json,
                Err(err) => {
                    log::error!(log::channel!("ui"), "Can't read the language file {path}: {err}");
                    continue;
                }
            };

            match UiLanguage::parse(name, &json) {
                Ok((language, problems)) => {
                    for problem in problems {
                        log::error!(log::channel!("ui"), "{path}: {problem}");
                    }
                    languages.push(language);
                }
                Err(err) => log::error!(log::channel!("ui"), "Can't load the language file {path}: {err}"),
            }
        }

        let store = Self::new(languages);
        if store.english.is_none() {
            log::error!(log::channel!("ui"), "No English strings ({dir}/{ENGLISH}.json): every key shows as itself.");
        }
        store
    }

    fn language_index(&self, name: &str) -> Option<usize> {
        self.languages.iter().position(|language| language.name == name)
    }

    // The key's entry and the language it comes from: `current`, else English.
    fn find(&self, current: Option<usize>, category: UiTextCategory, key: UiTextKey) -> Option<(&UiTextEntry, &UiLanguage)> {
        let in_language = |index: usize| self.languages[index].entry(category, key).map(|entry| (entry, &self.languages[index]));
        current.and_then(in_language).or_else(|| self.english.and_then(in_language))
    }

    // The current language; None when no language loaded.
    fn current(&self) -> Option<usize> {
        let index = CURRENT_LANGUAGE.load(Ordering::Relaxed);
        if index < self.languages.len() { Some(index) } else { self.english }
    }
}

fn text_dir() -> AssetPath {
    config::configs_path().join("ui/text")
}

// ----------------------------------------------
// Global state
// ----------------------------------------------

static STORE: OnceLock<UiTextStore> = OnceLock::new();
static CURRENT_LANGUAGE: AtomicUsize = AtomicUsize::new(usize::MAX); // Index into the store's languages; English until set.
static TEXT_VERSION: AtomicU32 = AtomicU32::new(0);
static MISSES: Mutex<UiTextMisses> = Mutex::new(UiTextMisses { missing: BTreeMap::new(), untranslated: BTreeMap::new() });

// Loaded on first use.
pub(crate) fn store() -> &'static UiTextStore {
    STORE.get_or_init(UiTextStore::load)
}

// Keys looked up and not found.
struct UiTextMisses {
    missing: BTreeMap<UiTextCategory, BTreeSet<String>>, // Not in English either.
    untranslated: BTreeMap<(usize, UiTextCategory), BTreeSet<String>>, // By language (the store's index): in English only.
}

fn get_hashed(category: UiTextCategory, key: &str, hash: UiTextKey) -> &str {
    lookup(category, key, hash).map_or(key, |(entry, _)| entry.text())
}

// The key's entry and the language it comes from: the current one, else English (a fallback,
// logged once per key). None if English lacks it too (logged once per key, and recorded).
fn lookup(category: UiTextCategory, key: &str, hash: UiTextKey) -> Option<(&'static UiTextEntry, &'static UiLanguage)> {
    let store = store();
    let current = store.current();
    let found = store.find(current, category, hash);

    match (found, current) {
        (Some((_, language)), Some(index)) if !std::ptr::eq(language, &store.languages[index]) => {
            if record_miss(Some(index), category, key) {
                let current_name = &store.languages[index].name;
                log::warning!(log::channel!("ui"), "No {category} '{key}' in language '{current_name}': using English.");
            }
        }
        (Some(_), _) => {}
        (None, _) => {
            if record_miss(None, category, key) {
                log::warning!(log::channel!("ui"), "No {category} '{key}' in {ENGLISH}.json: showing the key.");
            }
        }
    }

    found
}

// Records a key missed in a language (the store's index) and found in English, or (None) missed
// in English too. True the first time, the only time the key is copied; a key on screen is looked
// up every frame. The lock is released before the caller logs.
fn record_miss(language: Option<usize>, category: UiTextCategory, key: &str) -> bool {
    let mut misses = MISSES.lock().unwrap_or_else(PoisonError::into_inner);
    let keys = match language {
        Some(index) => misses.untranslated.entry((index, category)).or_default(),
        None => misses.missing.entry(category).or_default(),
    };
    !keys.contains(key) && keys.insert(key.to_string())
}

// For the developer UI (engine::debug): the store and the misses, drawn by its DrawDebugUi.
pub(crate) struct UiTextDebug;

impl UiTextDebug {
    pub(crate) fn store(&self) -> &'static UiTextStore {
        store()
    }
}

// ----------------------------------------------
// Unit tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    fn language(name: &str, json: &str) -> UiLanguage {
        let (language, problems) = UiLanguage::parse(name, json).unwrap();
        assert!(problems.is_empty(), "{problems:?}");
        language
    }

    #[test]
    fn placeholders_are_filled_by_name() {
        let args = [("count", "3"), ("name", "Temple")];
        assert_eq!(fill_placeholders("{name}: {count} left", &args), "Temple: 3 left");
        assert_eq!(fill_placeholders("{{name}} and {{{count}}}", &args), "{name} and {3}");
        assert_eq!(fill_placeholders("{unknown} {name", &args), "{unknown} {name");
        assert_eq!(fill_placeholders("{ name } {}", &args), "{ name } {}");
        assert_eq!(fill_placeholders("Đại Long: {name}.", &args), "Đại Long: Temple.");
        assert_eq!(placeholder_names("{a} {{b}} {c_1} {bad name}"), BTreeSet::from(["a", "c_1"]));
    }

    #[test]
    fn plural_forms_follow_the_language_rule() {
        let english = PluralRule::for_language("en");
        assert_eq!(english.form(1), PluralForm::One);
        assert_eq!(english.form(-1), PluralForm::One);
        assert_eq!(english.form(0), PluralForm::Other);
        assert_eq!(english.form(2), PluralForm::Other);
        assert_eq!(PluralRule::for_language("vi").form(1), PluralForm::Other);

        let language = language("en", r#"{ "Hud": { "months": { "one": "{count} month", "other": "{count} months" } } }"#);
        let entry = language.entry(UiTextCategory::Hud, hash::fnv1a_from_str("months")).unwrap();
        assert_eq!(entry.form(PluralForm::One), "{count} month");
        assert_eq!(entry.form(PluralForm::Few), "{count} months"); // A form not given: `other`.
        assert_eq!(entry.text(), "{count} months");
    }

    #[test]
    fn bad_entries_are_reported_and_left_out() {
        let json = r#"{
            "Menu": { "ok": "Ok", "plural": { "one": "x" }, "form": { "single": "x", "other": "y" }, "number": 3 },
            "Nope": {},
            "Hud": []
        }"#;
        let (language, problems) = UiLanguage::parse("en", json).unwrap();
        assert_eq!(language.keys(), BTreeSet::from([(UiTextCategory::Menu, "ok")]));
        assert_eq!(problems.len(), 5, "{problems:?}");

        assert!(UiLanguage::parse("en", "[]").is_err());
        assert!(UiLanguage::parse("en", "{ broken").is_err());
    }

    #[test]
    fn duplicate_keys_are_reported() {
        let json = r#"{
            "Menu": { "yes": "Yes", "no": "No", "yes": "Yep", "months": { "one": "a", "one": "b", "other": "c" } },
            "Hud": { "x": [{ "a": 1, "a": 2 }] },
            "Hud": {}
        }"#;
        assert_eq!(duplicate_keys(json), vec!["Menu/yes", "Menu/months/one", "Hud"]);

        let (language, problems) = UiLanguage::parse("en", json).unwrap();
        assert_eq!(problems.len(), 3, "{problems:?}");
        let yes = language.entry(UiTextCategory::Menu, hash::fnv1a_from_str("yes")).unwrap();
        assert_eq!(yes.text(), "Yep");

        assert!(duplicate_keys(r#"{ "Menu": { "a": "x", "b": "x" }, "Hud": { "a": "x" } }"#).is_empty());
    }

    #[test]
    fn find_falls_back_to_english() {
        let store = UiTextStore::new(vec![
            language("xx", r#"{ "Menu": { "ok": "Okay" } }"#),
            language("en", r#"{ "Menu": { "ok": "Ok", "cancel": "Cancel" } }"#),
        ]);
        assert_eq!(store.english, Some(0));
        let xx = store.language_index("xx");
        let find = |key: &str| {
            store.find(xx, UiTextCategory::Menu, hash::fnv1a_from_str(key)).map(|(entry, language)| (entry.text(), language.name.as_str()))
        };
        assert_eq!(find("ok"), Some(("Okay", "xx")));
        assert_eq!(find("cancel"), Some(("Cancel", "en")));
        assert_eq!(find("missing"), None);
    }

    // A count picks its plural form by the rule of the language the text comes from: English's
    // for a key the current language lacks.
    #[test]
    fn plural_forms_follow_the_text_language() {
        let store = UiTextStore::new(vec![
            language("vi", r#"{ "Hud": { "days": { "other": "{count} ngày" } } }"#),
            language("en", r#"{ "Hud": { "days": { "one": "{count} day", "other": "{count} days" }, "months": { "one": "{count} month", "other": "{count} months" } } }"#),
        ]);
        let vi = store.language_index("vi");
        let format = |key: &str, count: i64, args: &[(&str, &str)]| {
            let (entry, language) = store.find(vi, UiTextCategory::Hud, hash::fnv1a_from_str(key)).unwrap();
            fill_count(plural_text(entry, language, count), count, args)
        };
        assert_eq!(format("days", 1, &[]), "1 ngày"); // Vietnamese has one form.
        assert_eq!(format("months", 1, &[]), "1 month"); // From English, by English's rule.
        assert_eq!(format("months", 3, &[]), "3 months");
        assert_eq!(format("months", 1, &[("count", "a")]), "a month"); // args win.
    }

    #[test]
    fn ui_string_literals_and_keys() {
        assert_eq!(UiString::from("Ok").resolve(), "Ok");
        assert_eq!(UiString::from(String::from("Ok")), UiString::literal("Ok"));
        assert!(UiString::default().is_empty());

        let mut text = UiString::key(UiTextCategory::Menu, "button.cancel");
        assert!(text.is_key() && !text.is_empty());
        text.set_literal("12 gold");
        assert_eq!(text.resolve(), "12 gold");
        text.clear();
        assert!(text.is_empty());

        let key = UiString::key(UiTextCategory::Menu, "button.cancel");
        assert_eq!(key.id_source(), "button.cancel"); // Not the text, which follows the language.
        assert_eq!(UiString::from("Ok").id_source(), "Ok");
    }

    #[test]
    fn a_miss_is_recorded_once() {
        // A key no test or file names, so no other test records it.
        let key = "test.a_miss_is_recorded_once";
        assert!(record_miss(None, UiTextCategory::Story, key));
        assert!(!record_miss(None, UiTextCategory::Story, key));
        assert!(missing_keys()[&UiTextCategory::Story].iter().any(|missed| missed == key));
    }

    // ----------------------
    // The key check:
    // ----------------------

    fn repo_path(relative: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(relative)
    }

    fn load_language_file(path: &Path) -> UiLanguage {
        let name = path.file_stem().unwrap().to_str().unwrap();
        let json = std::fs::read_to_string(path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
        let (language, problems) = UiLanguage::parse(name, &json).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
        assert!(problems.is_empty(), "{}: {problems:?}", path.display());
        language
    }

    // Each `UiTextCategory::<Name>, "<key>"` in a source: the keys the code names.
    fn named_keys(source: &str) -> Vec<(String, String)> {
        const PREFIX: &str = "UiTextCategory::";
        let mut keys = Vec::new();
        let mut rest = source;

        while let Some(pos) = rest.find(PREFIX) {
            rest = &rest[pos + PREFIX.len()..];
            let name_len = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(rest.len());
            let category = &rest[..name_len];

            let Some(after_comma) = rest[name_len..].trim_start().strip_prefix(',') else {
                continue;
            };
            let Some(literal) = after_comma.trim_start().strip_prefix('"') else {
                continue;
            };
            if let Some(end) = literal.find('"') {
                keys.push((category.to_string(), literal[..end].to_string()));
            }
        }

        keys
    }

    fn rust_sources(dir: &Path, files: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if !name.starts_with('.') && name != "target" {
                    rust_sources(&path, files);
                }
            } else if name.ends_with(".rs") {
                files.push(path);
            }
        }
    }

    #[test]
    fn named_keys_are_found() {
        let source = "text::get(UiTextCategory::Menu, \"a.b\"); UiString::key(\n    UiTextCategory::Hud,\n    \"c\",\n); \
                      find_str(UiTextCategory::TileDescription, hash); UiTextCategory::City";
        assert_eq!(named_keys(source), vec![("Menu".into(), "a.b".into()), ("Hud".into(), "c".into())]);
    }

    // Every key the code names exists in en.json. (This file is skipped: its tests name made-up keys.)
    #[test]
    fn every_key_the_code_names_is_in_english() {
        let english = load_language_file(&repo_path("assets/configs/ui/text/en.json"));

        let mut files = Vec::new();
        rust_sources(&repo_path("crates"), &mut files);
        assert!(files.len() > 100, "found only {} sources", files.len());

        let mut missing = Vec::new();
        let mut named = 0;
        for file in files {
            if file.ends_with("engine/src/ui/text.rs") {
                continue;
            }
            let source = std::fs::read_to_string(&file).unwrap();
            for (category_name, key) in named_keys(&source) {
                named += 1;
                let Ok(category) = UiTextCategory::from_str(&category_name) else {
                    missing.push(format!("{}: unknown category {category_name}", file.display()));
                    continue;
                };
                if english.entry(category, hash::fnv1a_from_str(&key)).is_none() {
                    missing.push(format!("{}: {category}/{key}", file.display()));
                }
            }
        }

        assert!(named > 0, "no keys found in the sources: has the way they're written changed?");
        assert!(missing.is_empty(), "keys missing from en.json:\n{}", missing.join("\n"));
    }

    // No language file has a key en.json lacks, and a translation names only its English text's
    // placeholders.
    #[test]
    fn every_language_has_only_english_keys() {
        let dir = repo_path("assets/configs/ui/text");
        let english = load_language_file(&dir.join("en.json"));
        let english_keys = english.keys();

        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            assert_eq!(path.extension().and_then(|ext| ext.to_str()), Some("json"), "{}: not a language file", path.display());
            let language = load_language_file(&path);

            let extra: Vec<_> = language.keys().difference(&english_keys).map(|(category, key)| format!("{category}/{key}")).collect();
            assert!(extra.is_empty(), "{}: keys en.json lacks: {extra:?}", path.display());

            for category in UiTextCategory::iter() {
                for (key, entry) in &language.strings(category).entries {
                    let english_entry = english.entry(category, hash::fnv1a_from_str(key)).unwrap();
                    let english_names: BTreeSet<&str> = english_entry.forms().iter().flat_map(|(_, text)| placeholder_names(text)).collect();
                    for (form, text) in entry.forms() {
                        let unknown: Vec<_> = placeholder_names(text).difference(&english_names).copied().collect();
                        let form = if form.is_empty() { String::new() } else { format!(" [{form}]") };
                        assert!(unknown.is_empty(), "{}: {category}/{key}{form}: placeholders English lacks: {unknown:?}", path.display());
                    }
                }
            }
        }
    }
}
