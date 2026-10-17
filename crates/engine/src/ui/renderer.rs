// Turns what the UI library drew into the renderer's UiDrawData (render/ui_draw.rs), and keeps the
// textures the library manages itself, such as its font atlas, in the TextureCache.

use smallvec::SmallVec;

use common::{Size, Vec2, format_fixed_string};

use crate::{
    log,
    render::{
        texture::{TextureCache, TextureHandle, TextureSettings},
        ui_draw::{UiClipRect, UiDrawCommand, UiDrawData, UiIndex, UiVertex},
    },
};

// ----------------------------------------------
// UiRenderer
// ----------------------------------------------

#[derive(Default)]
pub(super) struct UiRenderer {
    draw_data: UiDrawData,
    textures: UiManagedTextures,
}

impl UiRenderer {
    #[inline]
    pub fn draw_data(&self) -> &UiDrawData {
        &self.draw_data
    }

    #[inline]
    pub fn textures_mut(&mut self) -> &mut UiManagedTextures {
        &mut self.textures
    }

    // Rebuilds the draw data from ImGui's. Its textures are TextureHandles, packed.
    pub fn build_from_imgui(&mut self, imgui_draw_data: &imgui::DrawData, framebuffer_size: Size) {
        self.draw_data.clear();

        let origin = Vec2::from_array(imgui_draw_data.display_pos);
        let pixels_per_point = Vec2::from_array(imgui_draw_data.framebuffer_scale);

        for draw_list in imgui_draw_data.draw_lists() {
            let vtx_buffer = draw_list.vtx_buffer();
            let idx_buffer = draw_list.idx_buffer();

            if vtx_buffer.is_empty() || idx_buffer.is_empty() {
                continue; // Nothing to draw for this draw list.
            }

            let mesh = self.draw_data.add_mesh(
                vtx_buffer.iter().map(|vertex| UiVertex {
                    pos: vertex.pos,
                    uv: vertex.uv,
                    color: premultiply_alpha(vertex.col),
                }),
                idx_buffer.iter().map(|&index| UiIndex::from(index)),
            );

            for command in draw_list.commands() {
                match command {
                    imgui::DrawCmd::Elements { count, cmd_params } => {
                        if count == 0 {
                            continue;
                        }

                        let [min_x, min_y, max_x, max_y] = cmd_params.clip_rect;
                        let Some(clip_rect) = UiClipRect::from_points(
                            Vec2::new(min_x, min_y) - origin,
                            Vec2::new(max_x, max_y) - origin,
                            pixels_per_point,
                            framebuffer_size,
                        ) else {
                            continue; // Clipped away entirely.
                        };

                        self.draw_data.add_command(UiDrawCommand {
                            first_index: mesh.first_index + cmd_params.idx_offset as u32,
                            index_count: count as u32,
                            base_vertex: mesh.base_vertex + cmd_params.vtx_offset as u32,
                            texture: TextureHandle::unpack(cmd_params.texture_id.id()),
                            clip_rect,
                        });
                    }
                    // These are not required by our UI renderer.
                    imgui::DrawCmd::ResetRenderState => {
                        unimplemented!("Haven't implemented imgui::DrawCmd::ResetRenderState yet!");
                    }
                    imgui::DrawCmd::RawCallback { .. } => {
                        unimplemented!("Haven't implemented imgui::DrawCmd::RawCallback yet!");
                    }
                }
            }
        }
    }
}

// Converts an RGBA color with straight alpha, as ImGui's vertex colors are, to premultiplied alpha.
#[inline(always)] // Also in debug builds: it runs for every UI vertex.
fn premultiply_alpha([r, g, b, a]: [u8; 4]) -> [u8; 4] {
    if a == u8::MAX {
        return [r, g, b, a];
    }
    let multiply = |c: u8| ((u32::from(c) * u32::from(a) + 127) / 255) as u8; // Rounded.
    [multiply(r), multiply(g), multiply(b), a]
}

// ----------------------------------------------
// UiTextureStore
// ----------------------------------------------

// Where UiManagedTextures keeps its textures: the TextureCache, or a stand-in in tests.
pub(super) trait UiTextureStore {
    fn create_texture(&mut self, name: &str, size: Size, pixels: &[u8], settings: TextureSettings) -> TextureHandle;
    fn update_texture(&mut self, handle: TextureHandle, offset: [u32; 2], size: Size, pixels: &[u8]);
    fn release_texture(&mut self, handle: TextureHandle);
}

impl UiTextureStore for TextureCache {
    fn create_texture(&mut self, name: &str, size: Size, pixels: &[u8], settings: TextureSettings) -> TextureHandle {
        // Explicit settings: the global texture settings don't apply to UI textures.
        self.new_initialized_texture(name, size, pixels, Some(settings))
    }

    fn update_texture(&mut self, handle: TextureHandle, offset: [u32; 2], size: Size, pixels: &[u8]) {
        self.update_texture(handle, offset[0], offset[1], size, 0, pixels);
    }

    fn release_texture(&mut self, mut handle: TextureHandle) {
        self.release_texture(&mut handle);
    }
}

// ----------------------------------------------
// UiManagedTextures
// ----------------------------------------------

// New pixels for a managed texture: all of it, or a part of it.
pub(super) struct UiTextureDelta<'a> {
    pub pos: Option<[u32; 2]>, // Top-left corner of the part to replace. None replaces the whole texture, and resizes it.
    pub size: Size,
    pub pixels: &'a [u8], // RGBA8 with straight alpha, rows from top to bottom.
    pub settings: TextureSettings,
}

struct UiManagedTexture {
    id: u64, // The UI library's own id for it (egui: TextureId::Managed).
    handle: TextureHandle,
    size: Size,
    settings: TextureSettings,
    recreated_count: u32, // Keeps the names of its replacements unique.
}

// The textures the UI library creates, updates and frees itself, such as its font atlas. Each frame,
// call release_freed first, then set for the frame's new pixels, then draw, then free for the
// textures the library is done with.
#[derive(Default)]
pub(super) struct UiManagedTextures {
    textures: SmallVec<[UiManagedTexture; 4]>,

    // Freed or replaced this frame. The frame may still draw them, so they're released at the
    // start of the next one, once this frame has been drawn.
    freed: SmallVec<[TextureHandle; 4]>,
}

impl UiManagedTextures {
    pub fn handle(&self, id: u64) -> Option<TextureHandle> {
        self.textures.iter().find(|texture| texture.id == id).map(|texture| texture.handle)
    }

    // Applies new pixels to a texture, creating it on its first full delta. A full delta of another
    // size or with other settings recreates the texture under a new handle, and the old one is
    // released a frame later. Returns the texture's handle, or an invalid handle if a partial delta
    // names an unknown texture.
    pub fn set(&mut self, store: &mut impl UiTextureStore, id: u64, delta: &UiTextureDelta) -> TextureHandle {
        debug_assert!(delta.size.is_valid());
        debug_assert_eq!(delta.pixels.len(), (delta.size.width * delta.size.height * 4) as usize); // RGBA8.

        let Some(index) = self.textures.iter().position(|texture| texture.id == id) else {
            if delta.pos.is_some() {
                log::error!(log::channel!("ui"), "UI texture {id} got a partial update before it was created.");
                return TextureHandle::invalid();
            }

            let name = format_fixed_string!(64, "ui_texture_{id}");
            let handle = store.create_texture(&name, delta.size, delta.pixels, delta.settings);

            self.textures.push(UiManagedTexture {
                id,
                handle,
                size: delta.size,
                settings: delta.settings,
                recreated_count: 0,
            });
            return handle;
        };

        let texture = &mut self.textures[index];

        match delta.pos {
            Some(pos) => {
                debug_assert!(pos[0] as i32 + delta.size.width <= texture.size.width);
                debug_assert!(pos[1] as i32 + delta.size.height <= texture.size.height);
                store.update_texture(texture.handle, pos, delta.size, delta.pixels);
            }
            None if delta.size == texture.size && delta.settings == texture.settings => {
                store.update_texture(texture.handle, [0, 0], delta.size, delta.pixels);
            }
            None => {
                // The old texture keeps its name until it's released, so the new one needs another.
                texture.recreated_count += 1;
                let name = format_fixed_string!(64, "ui_texture_{id}_{}", texture.recreated_count);

                self.freed.push(texture.handle);
                texture.handle = store.create_texture(&name, delta.size, delta.pixels, delta.settings);
                texture.size = delta.size;
                texture.settings = delta.settings;
            }
        }

        texture.handle
    }

    // Releases a texture once this frame has been drawn.
    pub fn free(&mut self, id: u64) {
        if let Some(index) = self.textures.iter().position(|texture| texture.id == id) {
            let texture = self.textures.swap_remove(index);
            self.freed.push(texture.handle);
        }
    }

    // Releases the textures freed or replaced during the previous frame, which has been drawn since.
    pub fn release_freed(&mut self, store: &mut impl UiTextureStore) {
        for handle in self.freed.drain(..) {
            store.release_texture(handle);
        }
    }
}

// ----------------------------------------------
// Unit tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::texture::{TextureFilter, TextureWrapMode};

    // Hands out handles in order and logs every call.
    #[derive(Default)]
    struct TestStore {
        created: u32,
        calls: Vec<String>,
    }

    impl UiTextureStore for TestStore {
        fn create_texture(&mut self, name: &str, size: Size, _pixels: &[u8], _settings: TextureSettings) -> TextureHandle {
            self.created += 1;
            self.calls.push(format!("create {name} {}x{} -> {}", size.width, size.height, self.created));
            TextureHandle::Index(self.created)
        }

        fn update_texture(&mut self, handle: TextureHandle, offset: [u32; 2], size: Size, _pixels: &[u8]) {
            self.calls.push(format!("update {handle:?} at {},{} {}x{}", offset[0], offset[1], size.width, size.height));
        }

        fn release_texture(&mut self, handle: TextureHandle) {
            self.calls.push(format!("release {handle:?}"));
        }
    }

    const LINEAR: TextureSettings =
        TextureSettings { filter: TextureFilter::Linear, wrap_mode: TextureWrapMode::ClampToEdge, mipmaps: false };

    fn full(width: i32, height: i32, pixels: &[u8]) -> UiTextureDelta<'_> {
        UiTextureDelta { pos: None, size: Size::new(width, height), pixels, settings: LINEAR }
    }

    fn take_calls(store: &mut TestStore) -> Vec<String> {
        std::mem::take(&mut store.calls)
    }

    #[test]
    fn creates_then_updates_in_place() {
        let mut store = TestStore::default();
        let mut textures = UiManagedTextures::default();
        let pixels = [255u8; 4 * 4 * 4];

        let handle = textures.set(&mut store, 7, &full(4, 4, &pixels));
        assert_eq!(handle, TextureHandle::Index(1));
        assert_eq!(textures.handle(7), Some(handle));
        assert_eq!(textures.handle(8), None);

        // A partial update, then a full one of the same size, keep the texture.
        let part = UiTextureDelta { pos: Some([1, 2]), size: Size::new(2, 1), pixels: &pixels[..8], settings: LINEAR };
        assert_eq!(textures.set(&mut store, 7, &part), handle);
        assert_eq!(textures.set(&mut store, 7, &full(4, 4, &pixels)), handle);

        textures.release_freed(&mut store);
        assert_eq!(take_calls(&mut store), [
            "create ui_texture_7 4x4 -> 1",
            "update Index(1) at 1,2 2x1",
            "update Index(1) at 0,0 4x4",
        ]);
    }

    #[test]
    fn recreates_on_resize_and_releases_the_old_texture_a_frame_later() {
        let mut store = TestStore::default();
        let mut textures = UiManagedTextures::default();
        let small = [0u8; 2 * 2 * 4];
        let large = [0u8; 4 * 2 * 4];

        textures.set(&mut store, 0, &full(2, 2, &small));
        let handle = textures.set(&mut store, 0, &full(4, 2, &large));
        assert_eq!(handle, TextureHandle::Index(2));
        assert_eq!(textures.handle(0), Some(handle));

        // Other settings recreate it too.
        let nearest = TextureSettings { filter: TextureFilter::Nearest, ..LINEAR };
        let delta = UiTextureDelta { settings: nearest, ..full(4, 2, &large) };
        assert_eq!(textures.set(&mut store, 0, &delta), TextureHandle::Index(3));

        assert_eq!(take_calls(&mut store), [
            "create ui_texture_0 2x2 -> 1",
            "create ui_texture_0_1 4x2 -> 2",
            "create ui_texture_0_2 4x2 -> 3",
        ]);

        // Next frame.
        textures.release_freed(&mut store);
        assert_eq!(take_calls(&mut store), ["release Index(1)", "release Index(2)"]);
    }

    #[test]
    fn frees_a_frame_later() {
        let mut store = TestStore::default();
        let mut textures = UiManagedTextures::default();
        let pixels = [0u8; 4];

        textures.set(&mut store, 1, &full(1, 1, &pixels));
        textures.set(&mut store, 2, &full(1, 1, &pixels));
        textures.free(1);
        textures.free(99); // Unknown: ignored.

        assert_eq!(textures.handle(1), None);
        assert_eq!(textures.handle(2), Some(TextureHandle::Index(2)));
        assert_eq!(take_calls(&mut store).len(), 2); // The two creates, no release yet.

        textures.release_freed(&mut store);
        textures.release_freed(&mut store); // Released once only.
        assert_eq!(take_calls(&mut store), ["release Index(1)"]);
    }

    #[test]
    fn partial_update_of_an_unknown_texture_is_ignored() {
        let mut store = TestStore::default();
        let mut textures = UiManagedTextures::default();
        let pixels = [0u8; 4];

        let delta = UiTextureDelta { pos: Some([0, 0]), size: Size::new(1, 1), pixels: &pixels, settings: LINEAR };
        assert_eq!(textures.set(&mut store, 3, &delta), TextureHandle::invalid());
        assert_eq!(textures.handle(3), None);
        assert!(store.calls.is_empty());
    }

    #[test]
    fn premultiplies_vertex_colors() {
        assert_eq!(premultiply_alpha([200, 100, 50, 255]), [200, 100, 50, 255]);
        assert_eq!(premultiply_alpha([200, 100, 50, 0]), [0, 0, 0, 0]);
        assert_eq!(premultiply_alpha([255, 255, 255, 128]), [128, 128, 128, 128]);
        assert_eq!(premultiply_alpha([200, 100, 51, 128]), [100, 50, 26, 128]);
    }
}
