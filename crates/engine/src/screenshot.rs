// Screenshots: saves frame captures (see render/capture.rs) as PNG images.
//
// Request one with Engine::request_screenshot. Players press PrintScreen or F12 (see
// match_hotkey), and `--screenshot-after-frames N` takes one and quits. The GPU readback runs on
// the main thread at the end of the frame (see render/capture.rs). On desktop the PNG is then
// encoded and written on a background thread, so the frame doesn't also wait for that. Web has
// no threads and no writable file system: it encodes in place, and "saving a file" hands the PNG
// to the browser as a download.

use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    thread::JoinHandle,
};

use common::{Size, Vec2};
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};

use crate::{
    app::input::{InputAction, InputKey, InputModifiers},
    file_sys::{
        self,
        paths::{self, FixedPath},
    },
    log,
    platform,
    render::capture::{CaptureId, CaptureResult, CapturedImage},
};

// ----------------------------------------------
// ScreenshotRequest / Screenshot
// ----------------------------------------------

pub type ScreenshotId = CaptureId;

pub enum ScreenshotOutput {
    File(PathBuf), // Write a PNG file (desktop; relative to paths::base_path()), or download it (web).
    Memory,        // Return the PNG bytes in Screenshot::png.
}

// Size of the saved image.
#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub enum ScreenshotScale {
    #[default]
    Pixels,      // Framebuffer resolution.
    Points,      // Window points: half the pixels each way on a 2x HiDPI screen. Matches input coordinates.
    Factor(f32), // Framebuffer resolution times this, from 0 (exclusive) to 1.
}

pub struct ScreenshotRequest {
    pub include_ui: bool, // False saves the scene only, without menus or other UI.
    pub scale: ScreenshotScale,
    pub output: ScreenshotOutput,
}

pub struct Screenshot {
    pub size: Size,            // In image pixels.
    pub scale: Vec2,           // Image pixels per window point. Above 1 on HiDPI screens, unless scaled down.
    pub path: Option<PathBuf>, // Set for ScreenshotOutput::File.
    pub png: Option<Vec<u8>>,  // Set for ScreenshotOutput::Memory.
}

pub type ScreenshotResult = Result<Screenshot, String>;

// ----------------------------------------------
// Hotkey
// ----------------------------------------------

pub struct ScreenshotHotkey {
    pub include_ui: bool,
}

// PrintScreen or F12 takes a screenshot; holding Shift leaves the UI out. PrintScreen fires on
// release because Windows reports only its release. F12 is there because Mac keyboards have no
// PrintScreen key. On web the PNG downloads, if the browser lets the key through.
pub fn match_hotkey(key: InputKey, action: InputAction, modifiers: InputModifiers) -> Option<ScreenshotHotkey> {
    if !is_hotkey(key) {
        return None;
    }

    let fires = match key {
        InputKey::PrintScreen => action == InputAction::Release,
        InputKey::F12 => action == InputAction::Press,
        _ => false,
    };

    fires.then(|| ScreenshotHotkey { include_ui: !modifiers.intersects(InputModifiers::Shift) })
}

// Keys match_hotkey reacts to. The engine forwards them to the run loop even while the UI has
// keyboard focus, so a screenshot can be taken with a dialog open.
#[inline]
pub fn is_hotkey(key: InputKey) -> bool {
    matches!(key, InputKey::PrintScreen | InputKey::F12)
}

// ----------------------------------------------
// Paths and encoding
// ----------------------------------------------

// Where screenshots taken with the hotkey go, next to saves/.
pub fn screenshots_path() -> FixedPath {
    paths::base_path().join("screenshots")
}

// Standard base64 with padding, e.g. for returning a PNG inside JSON.
pub fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let triple = [chunk[0], chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let bits = (u32::from(triple[0]) << 16) | (u32::from(triple[1]) << 8) | u32::from(triple[2]);

        out.push(TABLE[(bits >> 18) as usize & 63] as char);
        out.push(TABLE[(bits >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { TABLE[(bits >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[bits as usize & 63] as char } else { '=' });
    }
    out
}

pub fn encode_png(image: &CapturedImage) -> Result<Vec<u8>, String> {
    let width = image.size.width as u32;
    let height = image.size.height as u32;

    // PngEncoder panics on a size mismatch instead of returning an error.
    if image.pixels.len() != width as usize * height as usize * 4 {
        return Err(format!("Captured {} bytes for a {}x{} image.", image.pixels.len(), width, height));
    }

    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(&image.pixels, width, height, ExtendedColorType::Rgba8)
        .map_err(|err| format!("PNG encoding failed: {err}"))?;

    Ok(png)
}

// ----------------------------------------------
// Image comparison
// ----------------------------------------------

#[derive(Debug)]
pub enum ImageComparison {
    SizeMismatch { actual: Size, expected: Size },
    Compared {
        diff_pixels: u64,          // Pixels with a channel off by more than the tolerance.
        max_channel_diff: u8,      // The largest channel difference anywhere.
        diff_png: Option<Vec<u8>>, // If any pixels differ: those in red, over the dimmed image.
    },
}

// Compares two PNG images pixel by pixel. A pixel differs when one of its channels is off by more
// than `tolerance`. Meant for images from the same machine and render backend: GPUs and drivers
// render slightly differently.
pub fn compare_png(actual: &[u8], expected: &[u8], tolerance: u8) -> Result<ImageComparison, String> {
    let decode = |png: &[u8], what: &str| {
        image::load_from_memory_with_format(png, image::ImageFormat::Png)
            .map(|image| image.to_rgba8())
            .map_err(|err| format!("Can't decode the {what} PNG: {err}"))
    };

    let actual = decode(actual, "captured")?;
    let expected = decode(expected, "golden")?;

    let size_of = |image: &image::RgbaImage| Size::new(image.width() as i32, image.height() as i32);
    if actual.dimensions() != expected.dimensions() {
        return Ok(ImageComparison::SizeMismatch { actual: size_of(&actual), expected: size_of(&expected) });
    }

    let mut diff_pixels = 0;
    let mut max_channel_diff = 0;
    let mut diff_image = Vec::with_capacity(actual.as_raw().len());

    for (actual_pixel, expected_pixel) in actual.pixels().zip(expected.pixels()) {
        let pixel_diff = (0..4).map(|channel| actual_pixel[channel].abs_diff(expected_pixel[channel])).max().unwrap_or(0);
        max_channel_diff = max_channel_diff.max(pixel_diff);

        if pixel_diff > tolerance {
            diff_pixels += 1;
            diff_image.extend_from_slice(&[0xFF, 0, 0, 0xFF]);
        } else {
            let [r, g, b, _] = actual_pixel.0;
            diff_image.extend_from_slice(&[r / 3, g / 3, b / 3, 0xFF]);
        }
    }

    let diff_png = if diff_pixels > 0 {
        let image = CapturedImage { size: size_of(&actual), scale: Vec2::new(1.0, 1.0), pixels: diff_image };
        Some(encode_png(&image)?)
    } else {
        None
    };

    Ok(ImageComparison::Compared { diff_pixels, max_channel_diff, diff_png })
}

// Scales a capture down for ScreenshotScale::Points or Factor. Never scales up.
pub fn scale_image(image: CapturedImage, scale: ScreenshotScale) -> Result<CapturedImage, String> {
    let factor = match scale {
        ScreenshotScale::Pixels => return Ok(image),
        ScreenshotScale::Points => 1.0 / image.scale.x.max(image.scale.y).max(1.0),
        ScreenshotScale::Factor(factor) => factor,
    };

    if factor.is_nan() || factor <= 0.0 || factor >= 1.0 {
        return Ok(image);
    }

    let (width, height) = (image.size.width as u32, image.size.height as u32);
    let new_width = ((width as f32 * factor).round() as u32).max(1);
    let new_height = ((height as f32 * factor).round() as u32).max(1);

    let buffer = image::RgbaImage::from_raw(width, height, image.pixels)
        .ok_or_else(|| format!("Captured image data doesn't match its {width}x{height} size."))?;

    // Through DynamicImage, whose resize is compiled (optimized) in the image crate. The generic
    // imageops::resize would be compiled here, unoptimized in debug builds, and take seconds.
    let scaled = image::DynamicImage::ImageRgba8(buffer)
        .resize_exact(new_width, new_height, image::imageops::FilterType::Triangle)
        .into_rgba8();

    Ok(CapturedImage {
        size: Size::new(new_width as i32, new_height as i32),
        scale: Vec2::new(image.scale.x * new_width as f32 / width as f32, image.scale.y * new_height as f32 / height as f32),
        pixels: scaled.into_raw(),
    })
}

fn save_screenshot(image: CapturedImage, scale: ScreenshotScale, output: ScreenshotOutput) -> ScreenshotResult {
    let image = scale_image(image, scale)?;
    let png = encode_png(&image)?;
    let CapturedImage { size, scale, .. } = image;

    match output {
        ScreenshotOutput::File(path) => {
            write_png(&path, &png)?;
            Ok(Screenshot { size, scale, path: Some(path), png: None })
        }
        ScreenshotOutput::Memory => Ok(Screenshot { size, scale, path: None, png: Some(png) }),
    }
}

#[cfg(feature = "desktop")]
fn write_png(path: &Path, png: &[u8]) -> Result<(), String> {
    file_sys::create_path(path).map_err(|err| format!("Failed to create the folder for '{}': {err}", path.display()))?;
    file_sys::write_file(path, png).map_err(|err| format!("Failed to write '{}': {err}", path.display()))
}

// The browser saves it to its downloads, under the file name.
#[cfg(not(feature = "desktop"))]
fn write_png(path: &Path, png: &[u8]) -> Result<(), String> {
    let file_name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| "screenshot.png".into());
    platform::download_file(&file_name, png, "image/png")
}

// ----------------------------------------------
// ScreenshotSystem
// ----------------------------------------------

// Owned by the Engine. Matches finished captures with their requests and saves them.
pub(crate) struct ScreenshotSystem {
    waiting_for_capture: Vec<(ScreenshotId, ScreenshotScale, ScreenshotOutput)>,
    paths_in_use: Vec<(ScreenshotId, PathBuf)>, // Not yet written. New file names avoid them.
    writers: Vec<JoinHandle<()>>,
    results_sender: mpsc::Sender<(ScreenshotId, ScreenshotResult)>,
    results_receiver: mpsc::Receiver<(ScreenshotId, ScreenshotResult)>,
    finished: Vec<(ScreenshotId, ScreenshotResult)>,
}

impl ScreenshotSystem {
    pub fn new() -> Self {
        let (results_sender, results_receiver) = mpsc::channel();
        Self {
            waiting_for_capture: Vec::new(),
            paths_in_use: Vec::new(),
            writers: Vec::new(),
            results_sender,
            results_receiver,
            finished: Vec::new(),
        }
    }

    pub fn add_request(&mut self, id: ScreenshotId, scale: ScreenshotScale, output: ScreenshotOutput) {
        if let ScreenshotOutput::File(path) = &output {
            self.paths_in_use.push((id, path.clone()));
        }
        self.waiting_for_capture.push((id, scale, output));
    }

    pub fn on_capture_finished(&mut self, id: ScreenshotId, result: CaptureResult) {
        let Some(index) = self.waiting_for_capture.iter().position(|(waiting_id, ..)| *waiting_id == id) else {
            log::warning!(log::channel!("screenshot"), "Dropping capture {id:?}: no screenshot requested it.");
            return;
        };

        let (_, scale, output) = self.waiting_for_capture.swap_remove(index);

        match result {
            Ok(image) => self.save(id, image, scale, output),
            Err(err) => self.finish(id, Err(err)),
        }
    }

    #[cfg(feature = "desktop")]
    fn save(&mut self, id: ScreenshotId, image: CapturedImage, scale: ScreenshotScale, output: ScreenshotOutput) {
        let results_sender = self.results_sender.clone();
        let spawn_result = std::thread::Builder::new().name("screenshot".into()).spawn(move || {
            // The receiver only goes away when the engine shuts down, after joining this thread.
            let _ = results_sender.send((id, save_screenshot(image, scale, output)));
        });

        match spawn_result {
            Ok(writer) => self.writers.push(writer),
            Err(err) => self.finish(id, Err(format!("Failed to start the screenshot writer thread: {err}"))),
        }
    }

    #[cfg(not(feature = "desktop"))]
    fn save(&mut self, id: ScreenshotId, image: CapturedImage, scale: ScreenshotScale, output: ScreenshotOutput) {
        let result = save_screenshot(image, scale, output);
        self.finish(id, result);
    }

    fn finish(&mut self, id: ScreenshotId, result: ScreenshotResult) {
        self.paths_in_use.retain(|(in_use_id, _)| *in_use_id != id);
        self.finished.push((id, result));
    }

    // Collects the results of background writes. Call once per frame.
    pub fn update(&mut self) {
        while let Ok((id, result)) = self.results_receiver.try_recv() {
            self.finish(id, result);
        }
        self.writers.retain(|writer| !writer.is_finished());
    }

    // Waits for the background writes still running, then collects their results.
    pub fn wait_for_writes(&mut self) {
        for writer in self.writers.drain(..) {
            let _ = writer.join(); // Each writer sends its result before it ends.
        }
        self.update();
    }

    pub fn take_result(&mut self, id: ScreenshotId) -> Option<ScreenshotResult> {
        let index = self.finished.iter().position(|(finished_id, _)| *finished_id == id)?;
        Some(self.finished.swap_remove(index).1)
    }

    // A file path for a new screenshot in screenshots_path(), named after `name` and the local
    // time, e.g. "Heritage Builder 2026-10-14 09-30-05.png". Adds " (2)", " (3)", ... when the
    // file exists or another screenshot is about to be written to it.
    pub fn new_screenshot_path(&self, name: &str) -> PathBuf {
        let stem = match platform::local_date_time() {
            Some(now) => format!(
                "{name} {:04}-{:02}-{:02} {:02}-{:02}-{:02}",
                now.year, now.month, now.day, now.hour, now.minute, now.second
            ),
            None => name.to_string(),
        };

        let screenshots_path = screenshots_path();
        let screenshots_dir = screenshots_path.as_path();

        (1..)
            .map(|n| if n == 1 { format!("{stem}.png") } else { format!("{stem} ({n}).png") })
            .map(|file_name| screenshots_dir.join(file_name))
            .find(|path| !self.is_path_in_use(path) && !file_sys::exists(path))
            .unwrap()
    }

    fn is_path_in_use(&self, path: &Path) -> bool {
        self.paths_in_use.iter().any(|(_, in_use)| in_use == path)
    }
}

impl Drop for ScreenshotSystem {
    fn drop(&mut self) {
        // Let pending writes finish, so quitting right after a screenshot doesn't truncate it.
        for writer in self.writers.drain(..) {
            let _ = writer.join();
        }
    }
}

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_image(width: i32, height: i32) -> CapturedImage {
        let mut pixels = Vec::new();
        for i in 0..(width * height) {
            pixels.extend_from_slice(&[i as u8, 0x80, 0xFF - i as u8, 0xFF]);
        }
        CapturedImage { size: Size::new(width, height), scale: Vec2::new(1.0, 1.0), pixels }
    }

    #[test]
    fn test_hotkeys() {
        let none = InputModifiers::empty();

        assert!(match_hotkey(InputKey::PrintScreen, InputAction::Release, none).is_some_and(|hotkey| hotkey.include_ui));
        assert!(match_hotkey(InputKey::PrintScreen, InputAction::Press, none).is_none());
        assert!(match_hotkey(InputKey::F12, InputAction::Press, none).is_some_and(|hotkey| hotkey.include_ui));
        assert!(match_hotkey(InputKey::F12, InputAction::Repeat, none).is_none());
        assert!(match_hotkey(InputKey::F12, InputAction::Release, none).is_none());
        assert!(match_hotkey(InputKey::F11, InputAction::Press, none).is_none());

        let shift = match_hotkey(InputKey::F12, InputAction::Press, InputModifiers::Shift);
        assert!(shift.is_some_and(|hotkey| !hotkey.include_ui));

        assert!(is_hotkey(InputKey::PrintScreen) && is_hotkey(InputKey::F12) && !is_hotkey(InputKey::A));
    }

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_encode(&[0xff, 0xfe, 0x00]), "//4A");
    }

    #[test]
    fn test_encode_png_round_trip() {
        let image = test_image(3, 2);
        let png = encode_png(&image).unwrap();

        let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (3, 2));
        assert_eq!(decoded.into_raw(), image.pixels);
    }

    #[test]
    fn test_compare_png() {
        let image = test_image(3, 2);
        let png = encode_png(&image).unwrap();

        let Ok(ImageComparison::Compared { diff_pixels: 0, max_channel_diff: 0, diff_png: None }) = compare_png(&png, &png, 0)
        else {
            panic!("An image should match itself.");
        };

        // One pixel off by 4 in one channel: within a tolerance of 4, not of 3.
        let mut changed = test_image(3, 2);
        changed.pixels[5] += 4;
        let changed_png = encode_png(&changed).unwrap();

        assert!(matches!(compare_png(&changed_png, &png, 4), Ok(ImageComparison::Compared { diff_pixels: 0, .. })));

        let Ok(ImageComparison::Compared { diff_pixels: 1, max_channel_diff: 4, diff_png: Some(diff_png) }) =
            compare_png(&changed_png, &png, 3)
        else {
            panic!("Expected one differing pixel.");
        };
        let diff = image::load_from_memory(&diff_png).unwrap().to_rgba8();
        assert_eq!(diff.get_pixel(1, 0).0, [0xFF, 0, 0, 0xFF]); // The changed pixel, in red.
        assert_ne!(diff.get_pixel(0, 0).0, [0xFF, 0, 0, 0xFF]);

        let smaller = encode_png(&test_image(2, 2)).unwrap();
        assert!(matches!(compare_png(&smaller, &png, 0), Ok(ImageComparison::SizeMismatch { .. })));
        assert!(compare_png(b"not a png", &png, 0).unwrap_err().contains("captured"));
    }

    #[test]
    fn test_scale_image() {
        let mut hidpi = test_image(8, 4);
        hidpi.scale = Vec2::new(2.0, 2.0);

        let points = scale_image(hidpi, ScreenshotScale::Points).unwrap();
        assert_eq!(points.size, Size::new(4, 2));
        assert_eq!(points.scale, Vec2::new(1.0, 1.0));
        assert_eq!(points.pixels.len(), 4 * 2 * 4);

        let quarter = scale_image(test_image(8, 4), ScreenshotScale::Factor(0.25)).unwrap();
        assert_eq!(quarter.size, Size::new(2, 1));

        // Never scales up, and Points is a no-op without HiDPI.
        assert_eq!(scale_image(test_image(8, 4), ScreenshotScale::Factor(2.0)).unwrap().size, Size::new(8, 4));
        assert_eq!(scale_image(test_image(8, 4), ScreenshotScale::Points).unwrap().size, Size::new(8, 4));
    }

    #[test]
    fn test_encode_png_rejects_wrong_size() {
        let mut image = test_image(2, 2);
        image.pixels.pop();
        assert!(encode_png(&image).is_err());
    }

    #[test]
    fn test_save_to_memory_and_file() {
        let screenshot = save_screenshot(test_image(2, 2), ScreenshotScale::Pixels, ScreenshotOutput::Memory).unwrap();
        assert_eq!(screenshot.size, Size::new(2, 2));
        assert!(screenshot.path.is_none() && screenshot.png.is_some());

        let dir = std::env::temp_dir().join(format!("heritage_screenshot_test_{}", std::process::id()));
        let path = dir.join("nested").join("shot.png");

        let screenshot =
            save_screenshot(test_image(4, 1), ScreenshotScale::Pixels, ScreenshotOutput::File(path.clone())).unwrap();
        assert_eq!(screenshot.path.as_deref(), Some(path.as_path()));
        assert_eq!(image::open(&path).unwrap().to_rgba8().dimensions(), (4, 1));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_system_matches_captures_with_requests() {
        let mut system = ScreenshotSystem::new();

        let mut queue = crate::render::capture::CaptureQueue::default();
        let id = queue.request(crate::render::capture::CaptureRequest { include_ui: true });
        let unknown = queue.request(crate::render::capture::CaptureRequest { include_ui: true });

        system.add_request(id, ScreenshotScale::Pixels, ScreenshotOutput::Memory);
        system.on_capture_finished(unknown, Ok(test_image(1, 1))); // Ignored.
        system.on_capture_finished(id, Ok(test_image(1, 1)));

        // Desktop saves on a background thread.
        let result = loop {
            system.update();
            if let Some(result) = system.take_result(id) {
                break result;
            }
            std::thread::yield_now();
        };

        assert!(result.unwrap().png.is_some());
        assert!(system.take_result(id).is_none());
        assert!(system.take_result(unknown).is_none());
    }

    #[test]
    fn test_capture_errors_are_reported() {
        let mut system = ScreenshotSystem::new();
        let mut queue = crate::render::capture::CaptureQueue::default();
        let id = queue.request(crate::render::capture::CaptureRequest { include_ui: false });

        system.add_request(id, ScreenshotScale::Pixels, ScreenshotOutput::File(PathBuf::from("unused.png")));
        assert!(system.is_path_in_use(Path::new("unused.png")));

        system.on_capture_finished(id, Err("GPU on fire".into()));
        assert_eq!(system.take_result(id).map(|result| result.err()), Some(Some("GPU on fire".to_string())));
        assert!(!system.is_path_in_use(Path::new("unused.png")));
    }
}
