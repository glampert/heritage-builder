// Turns what egui drew into the renderer's UiDrawData (render/ui_draw.rs), and keeps the textures
// egui manages itself, such as its font atlas, in the TextureCache.

use smallvec::SmallVec;

use egui::{TextureId, TexturesDelta, epaint::{ClippedPrimitive, Primitive}};

use common::{Size, Vec2, format_fixed_string};

use crate::{
    log,
    render::{
        texture::{TextureCache, TextureFilter, TextureHandle, TextureSettings, TextureWrapMode},
        ui_draw::{UiClipRect, UiDrawCommand, UiDrawData, UiVertex},
    },
};

// egui's meshes are copied into UiDrawData as they are.
const _: () = {
    use std::mem::{align_of, offset_of, size_of};
    assert!(size_of::<egui::epaint::Vertex>() == size_of::<UiVertex>());
    assert!(align_of::<egui::epaint::Vertex>() == align_of::<UiVertex>());
    assert!(offset_of!(egui::epaint::Vertex, pos) == offset_of!(UiVertex, pos));
    assert!(offset_of!(egui::epaint::Vertex, uv) == offset_of!(UiVertex, uv));
    assert!(offset_of!(egui::epaint::Vertex, color) == offset_of!(UiVertex, color));
};

// ----------------------------------------------
// UiRenderer
// ----------------------------------------------

#[derive(Default)]
pub(super) struct UiRenderer {
    draw_data: UiDrawData,
    textures: UiManagedTextures,
    pixels: Vec<u8>, // Texture updates in straight alpha, reused.
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

    // Applies egui's new texture pixels (`delta.set`), before drawing.
    pub fn apply_texture_updates(&mut self, store: &mut impl UiTextureStore, delta: &mut TexturesDelta) {
        for (id, image_deltas) in delta.set.drain() {
            let TextureId::Managed(managed_id) = id else {
                log::error!(log::channel!("ui"), "egui updated a texture it doesn't manage: {id:?}.");
                continue;
            };

            for image_delta in image_deltas {
                let egui::ImageData::Color(image) = &image_delta.image;

                // egui's colors have premultiplied alpha; textures are stored with straight alpha.
                self.pixels.clear();
                self.pixels.extend(image.pixels.iter().flat_map(|color| color.to_srgba_unmultiplied()));

                self.textures.set(store, managed_id, &UiTextureDelta {
                    pos: image_delta.pos.map(|[x, y]| [x as u32, y as u32]),
                    size: Size::new(image.size[0] as i32, image.size[1] as i32),
                    pixels: &self.pixels,
                    settings: texture_settings(image_delta.options),
                });
            }
        }
    }

    // Releases the textures egui is done with (`delta.free`) a frame later, once they've been drawn.
    pub fn free_textures(&mut self, delta: &mut TexturesDelta) {
        for id in delta.free.drain() {
            if let TextureId::Managed(managed_id) = id {
                self.textures.free(managed_id);
            }
        }
    }

    // Rebuilds the draw data from egui's tessellated meshes.
    pub fn build_draw_data(&mut self, primitives: &[ClippedPrimitive], pixels_per_point: f32, framebuffer_size: Size) {
        self.draw_data.clear();

        let scale = Vec2::new(pixels_per_point, pixels_per_point);

        for ClippedPrimitive { clip_rect, primitive } in primitives {
            let Primitive::Mesh(mesh) = primitive else {
                continue; // Paint callbacks: egui's way to draw with the renderer directly. Not used.
            };

            if mesh.indices.is_empty() {
                continue;
            }

            let Some(clip_rect) = UiClipRect::from_points(
                Vec2::new(clip_rect.min.x, clip_rect.min.y),
                Vec2::new(clip_rect.max.x, clip_rect.max.y),
                scale,
                framebuffer_size,
            ) else {
                continue; // Clipped away entirely.
            };

            let texture = match mesh.texture_id {
                TextureId::User(packed) => TextureHandle::unpack(packed as usize),
                TextureId::Managed(id) => self.textures.handle(id).unwrap_or_default(),
            };

            let start = self.draw_data.add_mesh(bytemuck::cast_slice(&mesh.vertices), &mesh.indices);

            self.draw_data.add_command(UiDrawCommand {
                first_index: start.first_index,
                index_count: mesh.indices.len() as u32,
                base_vertex: start.base_vertex,
                texture,
                clip_rect,
            });
        }
    }
}

// The texture settings egui asks for. Mipmaps aren't used for UI textures.
fn texture_settings(options: egui::TextureOptions) -> TextureSettings {
    let filter = match options.minification {
        egui::TextureFilter::Nearest => TextureFilter::Nearest,
        egui::TextureFilter::Linear => TextureFilter::Linear,
    };

    let wrap_mode = match options.wrap_mode {
        egui::TextureWrapMode::ClampToEdge => TextureWrapMode::ClampToEdge,
        egui::TextureWrapMode::Repeat | egui::TextureWrapMode::MirroredRepeat => TextureWrapMode::Repeat,
    };

    TextureSettings { filter, wrap_mode, mipmaps: false }
}

// The id a TextureHandle has in egui's meshes and images.
#[inline]
pub(super) fn egui_texture(texture: TextureHandle) -> TextureId {
    TextureId::User(texture.pack() as u64)
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
}
