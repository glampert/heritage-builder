// Screenshots: saves frame captures (see render/capture.rs) as PNG images.
//
// Request one with Engine::request_screenshot. Players press PrintScreen or F12 (see
// match_hotkey), and `--screenshot-after-frames N` takes one and quits. The GPU readback runs on
// the main thread at the end of the frame (see render/capture.rs). On desktop the PNG is then
// encoded and written on a background thread, so the frame doesn't also wait for that. Web has
// no threads and no writable file system: it encodes in place and can only return the PNG bytes.

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
    File(PathBuf), // Write a PNG file. Relative paths are relative to paths::base_path().
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
// PrintScreen key. Desktop only for now: the web build can't save files yet.
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
    cfg!(feature = "desktop") && matches!(key, InputKey::PrintScreen | InputKey::F12)
}

// ----------------------------------------------
// Paths and encoding
// ----------------------------------------------

// Where screenshots taken with the hotkey go, next to saves/.
pub fn screenshots_path() -> FixedPath {
    paths::base_path().join("screenshots")
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
    let scaled = image::imageops::resize(&buffer, new_width, new_height, image::imageops::FilterType::Triangle);

    Ok(CapturedImage {
        size: Size::new(new_width as i32, new_height as i32),
        scale: Vec2::new(image.scale.x * new_width as f32 / width as f32, image.scale.y * new_height as f32 / height as f32),
        pixels: scaled.into_raw(),
    })
}

fn save_screenshot(image: CapturedImage, scale: ScreenshotScale, output: ScreenshotOutput) -> ScreenshotResult {
    if cfg!(not(feature = "desktop")) && matches!(output, ScreenshotOutput::File(_)) {
        return Err("Saving screenshots to files isn't supported on the web build yet.".into());
    }

    let image = scale_image(image, scale)?;
    let png = encode_png(&image)?;
    let CapturedImage { size, scale, .. } = image;

    match output {
        ScreenshotOutput::File(path) => {
            file_sys::create_path(&path)
                .map_err(|err| format!("Failed to create the folder for '{}': {err}", path.display()))?;
            file_sys::write_file(&path, &png).map_err(|err| format!("Failed to write '{}': {err}", path.display()))?;

            Ok(Screenshot { size, scale, path: Some(path), png: None })
        }
        ScreenshotOutput::Memory => Ok(Screenshot { size, scale, path: None, png: Some(png) }),
    }
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
    fn test_encode_png_round_trip() {
        let image = test_image(3, 2);
        let png = encode_png(&image).unwrap();

        let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (3, 2));
        assert_eq!(decoded.into_raw(), image.pixels);
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
