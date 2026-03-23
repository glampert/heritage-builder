#![allow(clippy::needless_range_loop)]

#[cfg(feature = "desktop")]
use rayon::prelude::*;

use image::RgbaImage;
use serde::{Serialize, Deserialize};

use super::{sets::TileTexInfo, TileMapLayerKind};
use crate::{
    log,
    save::{self, SaveState},
    render::{TextureCache, TextureHandle, TextureSettings},
    file_sys::{self, paths::{self, PathRef, FixedPath}},
    utils::{
        Size,
        RectTexCoords,
        fixed_string::format_fixed_string,
        hash::{self, StringHash, PreHashedKeyMap},
    },
};

// ----------------------------------------------
// TextureAtlas
// ----------------------------------------------

pub trait TextureAtlas {
    fn load_texture(&mut self, tex_cache: &mut dyn TextureCache, texture_path: PathRef) -> TileTexInfo;
    fn commit_textures(&self, tex_cache: &mut dyn TextureCache);
    fn save_textures_to_file(&self, base_path: PathRef);
}

// ----------------------------------------------
// PassthroughTextureAtlas
// ----------------------------------------------

// No-op implementation that doesn't build a texture atlas.
// Each texture is a standalone image/TextureHandle loaded on the spot.
// Using this implementation disables texture atlas packing.
pub struct PassthroughTextureAtlas {
    layer: TileMapLayerKind,
}

impl PassthroughTextureAtlas {
    #[inline]
    pub fn new(layer: TileMapLayerKind, _tex_cache: &mut dyn TextureCache) -> Self {
        Self { layer }
    }
}

impl TextureAtlas for PassthroughTextureAtlas {
    #[inline]
    fn load_texture(&mut self, tex_cache: &mut dyn TextureCache, texture_path: PathRef) -> TileTexInfo {
        debug_assert!(!texture_path.is_empty());
        let file_path = paths::assets_path().join(texture_path);
        let texture = {
            // Terrain must always use nearest-neighbor filtering (default) to avoid seams.
            if self.layer == TileMapLayerKind::Terrain {
                tex_cache.load_texture_with_settings((&file_path).into(), Some(TextureSettings::default()))
            } else {
                tex_cache.load_texture((&file_path).into())
            }
        };

        TileTexInfo { texture, coords: RectTexCoords::default() }
    }

    #[inline]
    fn commit_textures(&self, _tex_cache: &mut dyn TextureCache) {}

    #[inline]
    fn save_textures_to_file(&self, _base_path: PathRef) {}
}

// ----------------------------------------------
// RuntimePackedTextureAtlas
// ----------------------------------------------

// Texture atlas built and packed at runtime, using the AtlasPacker.
// Packs textures with the help of the `texture_packer` crate using
// the "skyline" algorithm (see AtlasPacker below).
pub struct RuntimePackedTextureAtlas {
    layer: TileMapLayerKind,
    packer: packer::AtlasPacker,
}

impl RuntimePackedTextureAtlas {
    #[inline]
    pub fn new(layer: TileMapLayerKind, _tex_cache: &mut dyn TextureCache) -> Self {
        Self { layer, packer: packer::AtlasPacker::new(layer) }
    }
}

impl TextureAtlas for RuntimePackedTextureAtlas {
    fn load_texture(&mut self, tex_cache: &mut dyn TextureCache, texture_path: PathRef) -> TileTexInfo {
        debug_assert!(!texture_path.is_empty());
        if let Some(image) = load_image_file(paths::assets_path().into(), texture_path) {
            self.packer.pack_image(tex_cache, texture_path, image)
        } else {
            TileTexInfo::default()
        }
    }

    fn commit_textures(&self, tex_cache: &mut dyn TextureCache) {
        log::info!(log::channel!("atlas"),
                   "Committing texture atlas '{}' to graphics memory...",
                   self.layer);

        for page in self.packer.pages() {
            let (width, height) = page.image.dimensions();
            let pixels = page.image.as_raw();

            tex_cache.update_texture(page.texture,
                                     0,
                                     0,
                                     Size::new(width as i32, height as i32),
                                     0,
                                     pixels);
        }

        log::info!(log::channel!("atlas"),
                   "Texture atlas '{}' committed ({} pages).",
                   self.layer, self.packer.page_count());
    }

    #[cfg(feature = "desktop")]
    fn save_textures_to_file(&self, base_path: PathRef) {
        let save_path =
            paths::base_path()
                .join(base_path)
                .join("atlas")
                .join(self.layer.lowercase_name());

        let _ = file_sys::create_path(&save_path);

        log::info!(log::channel!("atlas"),
                   "Saving texture atlas {} with {} pages...",
                   save_path, self.packer.page_count());

        // Save pages in parallel:
        self.packer.pages()
            .par_iter()
            .enumerate()
            .for_each(|(index, page)| {
                let image_file_path = save_path
                    .join(format_fixed_string!(64, "page_{index}.png"));

                save_image_file((&image_file_path).into(), &page.image);

                let metadata_file_path = save_path
                    .join(format_fixed_string!(64, "page_{index}_sprite_meta.json"));

                let page_size = Size::new(page.image.width() as i32, page.image.height() as i32);
                save_sprite_metadata_file((&metadata_file_path).into(), &page.sprites, page_size);
            });

        let metadata_file_path = save_path.join("atlas_meta.json");
        save_atlas_metadata_file((&metadata_file_path).into(), self.packer.page_count(), self.layer);
    }

    #[cfg(feature = "web")]
    fn save_textures_to_file(&self, _base_path: PathRef) {
        // No-op on Web/WASM: atlas caching is a desktop optimization.
    }
}

// ----------------------------------------------
// OfflinePackedTextureAtlas
// ----------------------------------------------

// Texture atlas already pre-packed offline into a set of sprite
// sheet images and metadata, loaded from the cache directory.
pub struct OfflinePackedTextureAtlas {
    layer: TileMapLayerKind,
    pages: Vec<OfflinePackedAtlasPage>,
    mapping: PreHashedKeyMap<StringHash, (usize, usize)>, // sprite.key => (page_index, sprite_index)
}

impl OfflinePackedTextureAtlas {
    pub fn new(layer: TileMapLayerKind, tex_cache: &mut dyn TextureCache) -> Self {
        let layer_name = layer.lowercase_name();

        let cache_path =
            paths::base_path()
                .join(CACHE_BASE_PATH)
                .join("atlas")
                .join(layer_name);

        let atlas_metadata_path = cache_path.join("atlas_meta.json");
        let atlas_metadata = load_atlas_metadata_file((&atlas_metadata_path).into(), layer);

        let mut page_images = load_cached_atlas_page_images(layer_name, atlas_metadata.page_count);
        debug_assert!(page_images.len() == atlas_metadata.page_count);

        let mut pages = Vec::with_capacity(atlas_metadata.page_count);
        let mut mapping = PreHashedKeyMap::default();

        for page_index in 0..atlas_metadata.page_count {
            let sprite_metadata_path = cache_path
                .join(format_fixed_string!(64, "page_{page_index}_sprite_meta.json"));

            let sprite_metadata = load_sprite_metadata_file((&sprite_metadata_path).into());

            // Build mapping:
            for (sprite_index, sprite) in sprite_metadata.sprites.iter().enumerate() {
                if mapping.insert(sprite.key, (page_index, sprite_index)).is_some() {
                    log::error!(log::channel!("atlas"),
                                "Sprite key collision! Atlas {layer_name}, page [{page_index}], sprite [{sprite_index}]");
                }
            }

            // Convert image into texture:
            let texture = {
                let image = page_images[page_index].as_ref().unwrap();
                let pixels = image.as_raw();

                let tex_name = format_fixed_string!(64, "tex_atlas_{layer_name}_page_{page_index}");
                let tex_size = Size::new(image.width() as i32, image.height() as i32);

                if sprite_metadata.page_size != tex_size {
                    log::error!(log::channel!("atlas"),
                                "Page size mismatch: Expected {} but found texture of size {}, in atlas {}, page [{}]",
                                sprite_metadata.page_size, tex_size, layer_name, page_index);
                }

                if layer == TileMapLayerKind::Terrain {
                    // Terrain must always use nearest-neighbor filtering (default) to avoid seams.
                    tex_cache.new_initialized_texture(&tex_name, tex_size, pixels, Some(TextureSettings::default()))
                } else {
                    tex_cache.new_initialized_texture(&tex_name, tex_size, pixels, None)
                }
            };

            // We're done with this image, it's converted into a texture.
            // Drop it now so we can release some memory.
            page_images[page_index] = None;

            pages.push(OfflinePackedAtlasPage { texture, sprites: sprite_metadata.sprites });
        }

        Self { layer, pages, mapping }
    }
}

impl TextureAtlas for OfflinePackedTextureAtlas {
    fn load_texture(&mut self, _tex_cache: &mut dyn TextureCache, texture_path: PathRef) -> TileTexInfo {
        debug_assert!(!texture_path.is_empty());
        let key = hash::fnv1a_from_str(texture_path.as_str());

        self.mapping.get(&key).map_or(TileTexInfo::default(), |(page_index, sprite_index)| {
            let page = &self.pages[*page_index];
            let sprite = &page.sprites[*sprite_index];
    
            debug_assert!(sprite.key  == key);
            debug_assert!(sprite.path == texture_path.as_str());

            TileTexInfo { texture: page.texture, coords: sprite.rect }
        })
    }

    #[inline]
    fn commit_textures(&self, _tex_cache: &mut dyn TextureCache) {}

    #[inline]
    fn save_textures_to_file(&self, _base_path: PathRef) {}
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

pub const CACHE_BASE_PATH: PathRef = PathRef::from_str("cache");

pub fn cached_packed_atlas_exists(layer: TileMapLayerKind) -> bool {
    let layer_name = layer.lowercase_name();
    let cache_path = 
        paths::base_path()
            .join(CACHE_BASE_PATH)
            .join("atlas")
            .join(layer_name);
    cache_path.exists()
}

struct OfflinePackedAtlasPage {
    texture: TextureHandle,
    sprites: Vec<packer::AtlasSprite>,
}

fn load_cached_atlas_page_images(layer_name: &str, page_count: usize) -> Vec<Option<RgbaImage>> {
    // Load pages (parallel on desktop, sequential on Web/WASM):
    #[cfg(feature = "desktop")]
    let iter = (0..page_count).into_par_iter();
    #[cfg(feature = "web")]
    let iter = (0..page_count).into_iter();

    iter.map(|page_index| {
            let image_path: FixedPath =
                CACHE_BASE_PATH
                    .join("atlas")
                    .join(layer_name)
                    .join(format_fixed_string!(64, "page_{page_index}.png"));

            load_image_file(paths::base_path().into(), (&image_path).into())
                // Dummy 8x8 image fallback - all pixels = 0.
                .or_else(|| Some(RgbaImage::new(8, 8)))
        })
        .collect()
}

fn load_image_file(base_path: PathRef, path: PathRef) -> Option<RgbaImage> {
    let absolute_path: FixedPath = base_path.join(path);

    match file_sys::load_bytes(&absolute_path) {
        Ok(bytes) => {
            match image::load_from_memory(&bytes) {
                // Moves data, no pixel conversion if already RGBA8.
                Ok(image) => Some(image.into_rgba8()),
                Err(err) => {
                    log::error!(log::channel!("atlas"), "Failed to decode image file {absolute_path}: {err:?}");
                    None
                }
            }
        }
        Err(err) => {
            log::error!(log::channel!("atlas"), "Failed to load image file {absolute_path}: {err}");
            None
        }
    }
}

#[cfg(feature = "desktop")]
fn save_image_file(path: PathRef, image: &RgbaImage) -> bool {
    let mut file = match std::fs::File::create(path) {
        Ok(file) => file,
        Err(err) => {
            log::error!(log::channel!("atlas"), "Failed to create file {path}: {err:?}");
            return false;
        }
    };

    if let Err(err) = image.write_to(&mut file, image::ImageFormat::Png) {
        log::error!(log::channel!("atlas"), "Failed to write image file {path}: {err:?}");
        return false;
    }

    true
}

#[derive(Serialize)]
struct SerializedSpriteMetadata<'a> {
    page_size: Size,
    sprite_count: usize,
    sprites: &'a [packer::AtlasSprite],
}

fn save_sprite_metadata_file(path: PathRef, sprites: &[packer::AtlasSprite], page_size: Size) -> bool {
    let metadata = SerializedSpriteMetadata {
        page_size,
        sprite_count: sprites.len(),
        sprites,
    };

    let mut state = save::new_json_save_state(true);

    if let Err(err) = state.save(&metadata) {
        log::error!(log::channel!("atlas"), "Failed to save sprite metadata {path}: {err}");
        return false;
    }

    if let Err(err) = state.write_file(path) {
        log::error!(log::channel!("atlas"), "Failed to write sprite metadata file {path}: {err}");
        return false;
    }

    true
}

#[derive(Serialize)]
struct SerializedAtlasMetadata {
    page_count: usize,
    layer: TileMapLayerKind,
}

fn save_atlas_metadata_file(path: PathRef, page_count: usize, layer: TileMapLayerKind) -> bool {
    let metadata = SerializedAtlasMetadata {
        page_count,
        layer,
    };

    let mut state = save::new_json_save_state(true);

    if let Err(err) = state.save(&metadata) {
        log::error!(log::channel!("atlas"), "Failed to save atlas metadata {path}: {err}");
        return false;
    }

    if let Err(err) = state.write_file(path) {
        log::error!(log::channel!("atlas"), "Failed to write atlas metadata file {path}: {err}");
        return false;
    }

    true
}

#[derive(Deserialize, Default)]
struct DeserializedSpriteMetadata {
    page_size: Size,
    sprite_count: usize,
    sprites: Vec<packer::AtlasSprite>,
}

fn load_sprite_metadata_file(path: PathRef) -> DeserializedSpriteMetadata {
    let mut state = save::new_json_save_state(false);

    if let Err(err) = state.read_file(path) {
        log::error!(log::channel!("atlas"), "Failed to read sprite metadata file {path}: {err}");
        return DeserializedSpriteMetadata::default();
    }

    match state.load_new_instance::<DeserializedSpriteMetadata>() {
        Ok(metadata) => {
            if metadata.sprite_count != metadata.sprites.len() {
                log::error!(log::channel!("atlas"),
                            "Wrong sprite metadata count in {path}: Expected {} but found {}.",
                            metadata.sprite_count, metadata.sprites.len());
            }
            metadata
        }
        Err(err) => {
            log::error!(log::channel!("atlas"), "Failed to load sprite metadata {path}: {err}");
            DeserializedSpriteMetadata::default()
        }
    }
}

#[derive(Deserialize)]
struct DeserializedAtlasMetadata {
    page_count: usize,
    layer: TileMapLayerKind,
}

fn load_atlas_metadata_file(path: PathRef, layer: TileMapLayerKind) -> DeserializedAtlasMetadata {
    let mut state = save::new_json_save_state(false);

    if let Err(err) = state.read_file(path) {
        log::error!(log::channel!("atlas"), "Failed to read atlas metadata file {path}: {err}");
        return DeserializedAtlasMetadata { page_count: 0, layer };
    }

    match state.load_new_instance::<DeserializedAtlasMetadata>() {
        Ok(metadata) => {
            if metadata.layer != layer {
                log::error!(log::channel!("atlas"),
                            "Wrong atlas metadata layer in {path}: Expected {} but found {}.",
                            layer, metadata.layer);
            }
            metadata
        }
        Err(err) => {
            log::error!(log::channel!("atlas"), "Failed to load atlas metadata {path}: {err}");
            DeserializedAtlasMetadata { page_count: 0, layer }
        }
    }
}

// ----------------------------------------------
// AtlasPacker utilities
// ----------------------------------------------

mod packer {
    use super::*;
    use crate::utils::{Vec2, hash::{self, StringHash}};
    use texture_packer::{Frame, TexturePacker, TexturePackerConfig};

    const TEXTURE_PACKER_CONFIG: TexturePackerConfig = TexturePackerConfig {
        max_width: 4096,
        max_height: 4096,
        border_padding: 0,
        texture_padding: 2,
        texture_extrusion: 0,
        trim: false,
        allow_rotation: false,
        force_max_dimensions: true, // Force each page to max_width/max_height.
        texture_outlines: true,     // Debug bounding-box outlines.
    };

    type TexPacker = TexturePacker<'static, RgbaImage, StringHash>;

    #[derive(Serialize, Deserialize)]
    pub struct AtlasSprite {
        pub path: String,
        pub key: StringHash,     // Hash of the image path that was packed into the atlas.
        pub rect: RectTexCoords, // Coords inside the AtlasPage.
    }

    pub struct AtlasPage {
        pub image: RgbaImage,
        pub texture: TextureHandle,
        pub sprites: Vec<AtlasSprite>,
    }

    pub struct AtlasPacker {
        layer: TileMapLayerKind,
        pages: Vec<AtlasPage>,
        packers: Vec<TexPacker>,
    }

    impl AtlasPacker {
        pub fn new(layer: TileMapLayerKind) -> Self {
            Self { layer, pages: Vec::new(), packers: Vec::new() }
        }

        pub fn pack_image(&mut self,
                          tex_cache: &mut dyn TextureCache,
                          path: PathRef,
                          image: RgbaImage) -> TileTexInfo {
            debug_assert!(!path.is_empty());
            let key = hash::fnv1a_from_str(path.as_str());

            // Image fits an existing page?
            for (index, packer) in self.packers.iter_mut().enumerate() {
                if packer.can_pack(&image) {
                    return Self::do_pack_image(packer, &mut self.pages[index], key, path, image);
                }
            }

            // Need a new page:
            let (packer, page) = self.new_page(tex_cache);
            Self::do_pack_image(packer, page, key, path, image)
        }

        pub fn pages(&self) -> &[AtlasPage] {
           &self.pages 
        }

        pub fn page_count(&self) -> usize {
            self.pages.len()
        }

        fn new_page(&mut self, tex_cache: &mut dyn TextureCache) -> (&mut TexPacker, &mut AtlasPage) {
            let packer = TexPacker::new_skyline(TEXTURE_PACKER_CONFIG);

            // Allocate a new image/texture with the maximum page dimensions:
            let img_width  = TEXTURE_PACKER_CONFIG.max_width;
            let img_height = TEXTURE_PACKER_CONFIG.max_height;
            let image = RgbaImage::new(img_width, img_height);

            let tex_name = format_fixed_string!(64, "tex_atlas_{}_page_{}", self.layer.lowercase_name(), self.page_count());
            let tex_size = Size::new(img_width as i32, img_height as i32);

            let texture = {
                if self.layer == TileMapLayerKind::Terrain {
                    // Terrain must always use nearest-neighbor filtering (default) to avoid seams.
                    tex_cache.new_uninitialized_texture(&tex_name, tex_size, Some(TextureSettings::default()))
                } else {
                    tex_cache.new_uninitialized_texture(&tex_name, tex_size, None)
                }
            };

            self.pages.push(AtlasPage { image, texture, sprites: Vec::with_capacity(64) });
            self.packers.push(packer);

            (self.packers.last_mut().unwrap(), self.pages.last_mut().unwrap())
        }

        fn do_pack_image(packer: &mut TexPacker,
                         page: &mut AtlasPage,
                         key: StringHash,
                         path: PathRef,
                         image: RgbaImage) -> TileTexInfo
        {
            if let Err(err) = packer.pack_own(key, image) {
                log::error!(log::channel!("atlas"), "Failed to pack texture '{path}' into atlas: {err:?}");
                return TileTexInfo::default();
            }

            // Retrieve the frame metadata:
            let frame = packer.get_frame(&key).unwrap();
            let coords = rect_tex_coords_from_frame(frame);
            let texture = page.texture;

            // Update page's image data: blit this sprite image into the page texture.
            let sprite = packer.get_texture(&key).unwrap();
            Self::copy_packed_image_to_page(&mut page.image, sprite, frame);

            page.sprites.push(AtlasSprite { path: path.to_string(), key, rect: coords });

            TileTexInfo { texture, coords }
        }

        fn copy_packed_image_to_page(page: &mut RgbaImage, sprite: &RgbaImage, frame: &Frame<StringHash>) {
            let rect = &frame.frame;
            blit_rgba_image(page, sprite, rect.x as usize, rect.y as usize);
        }
    }

    fn rect_tex_coords_from_frame(frame: &Frame<StringHash>) -> RectTexCoords {
        let atlas_width  = TEXTURE_PACKER_CONFIG.max_width  as f32;
        let atlas_height = TEXTURE_PACKER_CONFIG.max_height as f32;
        let frame_rect  = frame.frame;

        // Compute normalized UV coordinates in [0, 1] range:
        let u0 = frame_rect.x as f32 / atlas_width;
        let u1 = (frame_rect.x + frame_rect.w) as f32 / atlas_width;

        // NOTE: Flip these to match OpenGL notation.
        let v1 = 1.0 - (frame_rect.y as f32 / atlas_height);
        let v0 = 1.0 - ((frame_rect.y + frame_rect.h) as f32 / atlas_height);

        RectTexCoords::new([
            Vec2::new(u0, v0), // top_left
            Vec2::new(u0, v1), // bottom_left
            Vec2::new(u1, v0), // top_right
            Vec2::new(u1, v1), // bottom_right
        ])
    }

    // Copy `src` into a sub-rectangle of `dst`, starting at (dst_x, dst_y).
    // Both images must be RGBA8.
    fn blit_rgba_image(dst: &mut RgbaImage, src: &RgbaImage, dst_x: usize, dst_y: usize) {
        let dst_width  = dst.width()  as usize;
        let dst_height = dst.height() as usize;

        let src_width  = src.width()  as usize;
        let src_height = src.height() as usize;

        assert!(dst_x + src_width  <= dst_width);
        assert!(dst_y + src_height <= dst_height);

        let dst_stride = dst_width * 4; // 4 bytes per pixel
        let src_stride = src_width * 4;

        let mut dst_samples = dst.as_flat_samples_mut();
        let src_samples = src.as_flat_samples();

        let dst_buf = dst_samples.as_mut_slice();
        let src_buf = src_samples.as_slice();

        for row in 0..src_height {
            let dst_row_start = ((dst_y + row) * dst_stride) + (dst_x * 4);
            let src_row_start = row * src_stride;
            let dst_row_end   = dst_row_start + src_stride;

            dst_buf[dst_row_start..dst_row_end].copy_from_slice(&src_buf[src_row_start..src_row_start + src_stride]);
        }
    }
}
