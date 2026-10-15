// Framebuffer capture, used for screenshots.
//
// A capture is requested during a frame and filled in by the render backend at the end of that
// frame, after the UI has drawn and before the frame is presented. OpenGL reads the pixels back
// right away. Wgpu copies them into a buffer that's mapped asynchronously: the result is ready at
// the end of the same frame on desktop, and a frame or more later on web.
//
// Use Engine::request_screenshot to save captures as PNG files. It owns the finished captures.

use common::{Rect, Size, Vec2};

// ----------------------------------------------
// CaptureRequest / CapturedImage
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct CaptureId(u64);

#[derive(Copy, Clone, Debug)]
pub struct CaptureRequest {
    pub include_ui: bool, // False captures the scene only, without menus or other UI.
}

#[derive(Clone)]
pub struct CapturedImage {
    pub size: Size,      // In framebuffer pixels.
    pub scale: Vec2,     // Framebuffer pixels per window point. Above 1 on HiDPI screens.
    pub pixels: Vec<u8>, // RGBA8, rows from top to bottom, fully opaque.
}

pub type CaptureResult = Result<CapturedImage, String>;

// ----------------------------------------------
// CaptureQueue
// ----------------------------------------------

// Requests made this frame, split by kind. A backend captures each kind at most once per frame
// and hands the image to every request of that kind.
#[derive(Default)]
pub(crate) struct PendingCaptures {
    pub scene: Vec<CaptureId>,
    pub with_ui: Vec<CaptureId>,
}

impl PendingCaptures {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.scene.is_empty() && self.with_ui.is_empty()
    }
}

#[derive(Default)]
pub(crate) struct CaptureQueue {
    next_id: u64,
    pending: PendingCaptures,                  // Captured at the end of the current frame.
    finished: Vec<(CaptureId, CaptureResult)>, // In completion order.
}

impl CaptureQueue {
    pub fn request(&mut self, request: CaptureRequest) -> CaptureId {
        self.next_id += 1;
        let id = CaptureId(self.next_id);

        if request.include_ui {
            self.pending.with_ui.push(id);
        } else {
            self.pending.scene.push(id);
        }

        id
    }

    #[inline]
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    #[inline]
    pub fn has_pending_with_ui(&self) -> bool {
        !self.pending.with_ui.is_empty()
    }

    #[inline]
    pub fn take_pending(&mut self) -> PendingCaptures {
        std::mem::take(&mut self.pending)
    }

    // Completes every request in `ids` with the same result.
    pub fn finish(&mut self, ids: &[CaptureId], result: CaptureResult) {
        if let Some((last, rest)) = ids.split_last() {
            for id in rest {
                self.finished.push((*id, result.clone()));
            }
            self.finished.push((*last, result));
        }
    }

    #[inline]
    pub fn take_finished(&mut self) -> Vec<(CaptureId, CaptureResult)> {
        std::mem::take(&mut self.finished)
    }
}

// ----------------------------------------------
// Pixel helpers for the backends
// ----------------------------------------------

// Framebuffer pixels per window point.
pub(super) fn content_scale(framebuffer_size: Size, viewport: Rect) -> Vec2 {
    if viewport.width() <= 0.0 || viewport.height() <= 0.0 {
        return Vec2::new(1.0, 1.0);
    }
    Vec2::new(framebuffer_size.width as f32 / viewport.width(), framebuffer_size.height as f32 / viewport.height())
}

// Reverses the row order in place. OpenGL reads the bottom row first.
pub(super) fn flip_rows(pixels: &mut [u8], row_bytes: usize) {
    debug_assert!(row_bytes != 0 && pixels.len().is_multiple_of(row_bytes));

    let rows = pixels.len() / row_bytes;
    for top in 0..rows / 2 {
        let bottom = rows - 1 - top;
        let (upper, lower) = pixels.split_at_mut(bottom * row_bytes);
        upper[top * row_bytes..(top + 1) * row_bytes].swap_with_slice(&mut lower[..row_bytes]);
    }
}

// Copies `rows` rows of `row_bytes` each out of a buffer whose rows are `padded_row_bytes` apart.
// Wgpu pads buffer copy rows to a multiple of 256 bytes.
pub(super) fn remove_row_padding(padded: &[u8], row_bytes: usize, padded_row_bytes: usize, rows: usize) -> Vec<u8> {
    debug_assert!(padded_row_bytes >= row_bytes);
    debug_assert!(padded.len() >= padded_row_bytes * rows.saturating_sub(1) + row_bytes);

    let mut pixels = Vec::with_capacity(row_bytes * rows);
    for row in 0..rows {
        let start = row * padded_row_bytes;
        pixels.extend_from_slice(&padded[start..start + row_bytes]);
    }
    pixels
}

// Swaps the red and blue channels of RGBA8 or BGRA8 pixels.
pub(super) fn swap_red_blue(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
}

// Sets every alpha value to 255. Blending leaves partial alpha in the framebuffer wherever
// translucent UI was drawn, but what's on screen is opaque.
pub(super) fn make_opaque(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
}

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_splits_requests_by_kind() {
        let mut queue = CaptureQueue::default();
        assert!(!queue.has_pending());

        let a = queue.request(CaptureRequest { include_ui: true });
        let b = queue.request(CaptureRequest { include_ui: false });
        let c = queue.request(CaptureRequest { include_ui: true });

        assert!(a != b && b != c && a != c);
        assert!(queue.has_pending() && queue.has_pending_with_ui());

        let pending = queue.take_pending();
        assert_eq!(pending.with_ui, vec![a, c]);
        assert_eq!(pending.scene, vec![b]);
        assert!(!queue.has_pending());
    }

    #[test]
    fn test_queue_finish_shares_one_result() {
        let mut queue = CaptureQueue::default();
        let a = queue.request(CaptureRequest { include_ui: true });
        let b = queue.request(CaptureRequest { include_ui: true });
        let pending = queue.take_pending();

        let image = CapturedImage { size: Size::new(1, 1), scale: Vec2::new(1.0, 1.0), pixels: vec![1, 2, 3, 4] };
        queue.finish(&pending.with_ui, Ok(image));
        queue.finish(&pending.scene, Err("unused".into())); // No requests: adds nothing.

        let finished = queue.take_finished();
        assert_eq!(finished.len(), 2);
        assert_eq!(finished[0].0, a);
        assert_eq!(finished[1].0, b);
        assert!(finished.iter().all(|(_, result)| result.as_ref().is_ok_and(|image| image.pixels == [1, 2, 3, 4])));
        assert!(queue.take_finished().is_empty());
    }

    #[test]
    fn test_flip_rows() {
        let mut pixels = vec![1, 1, 2, 2, 3, 3];
        flip_rows(&mut pixels, 2);
        assert_eq!(pixels, [3, 3, 2, 2, 1, 1]);

        let mut even = vec![1, 2, 3, 4];
        flip_rows(&mut even, 1);
        assert_eq!(even, [4, 3, 2, 1]);
    }

    #[test]
    fn test_remove_row_padding() {
        // Two rows of 3 bytes, padded to 5. The last row may be unpadded.
        let padded = [1, 2, 3, 0, 0, 4, 5, 6];
        assert_eq!(remove_row_padding(&padded, 3, 5, 2), [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn test_swap_red_blue_and_make_opaque() {
        let mut pixels = vec![10, 20, 30, 40, 50, 60, 70, 80];
        swap_red_blue(&mut pixels);
        assert_eq!(pixels, [30, 20, 10, 40, 70, 60, 50, 80]);

        make_opaque(&mut pixels);
        assert_eq!(pixels, [30, 20, 10, 255, 70, 60, 50, 255]);
    }

    #[test]
    fn test_content_scale() {
        let viewport = Rect::from_pos_and_size(Vec2::zero(), Vec2::new(640.0, 400.0));
        assert_eq!(content_scale(Size::new(1280, 800), viewport), Vec2::new(2.0, 2.0));
        assert_eq!(content_scale(Size::new(640, 400), Rect::default()), Vec2::new(1.0, 1.0));
    }
}
