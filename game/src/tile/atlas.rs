use std::fs;
use std::path::Path;
use std::fmt::Debug;
use image::{RgbaImage, ImageReader};

use super::{sets::TileTexInfo, TileMapLayerKind};
use crate::{
    log,
    render::{TextureCache, TextureHandle},
    utils::{RectTexCoords, Size},
};

// ----------------------------------------------
// TextureAtlas
// ----------------------------------------------

pub trait TextureAtlas {
    fn load_texture(&mut self, tex_cache: &mut dyn TextureCache, texture_path: &str) -> TileTexInfo;
    fn commit_textures(&self, tex_cache: &mut dyn TextureCache);
    fn save_textures_to_file(&self, base_path: &str);
}

// ----------------------------------------------
// PassthroughTextureAtlas
// ----------------------------------------------

// No-op implementation that doesn't build a texture atlas.
// Each texture is a standalone image and TextureHandle. 
pub struct PassthroughTextureAtlas;

impl PassthroughTextureAtlas {
    #[inline]
    pub fn new() -> Self { Self }
}

impl TextureAtlas for PassthroughTextureAtlas {
    #[inline]
    fn load_texture(&mut self, tex_cache: &mut dyn TextureCache, texture_path: &str) -> TileTexInfo {
        debug_assert!(!texture_path.is_empty());
        let texture = tex_cache.load_texture(texture_path);
        TileTexInfo { texture, coords: RectTexCoords::default() }
    }

    #[inline]
    fn commit_textures(&self, _tex_cache: &mut dyn TextureCache) {}

    #[inline]
    fn save_textures_to_file(&self, _base_path: &str) {}
}

// ----------------------------------------------
// PackedTextureAtlas
// ----------------------------------------------

pub struct PackedTextureAtlas {
    layer: TileMapLayerKind,
    packer: packer::AtlasPacker,
}

impl PackedTextureAtlas {
    pub fn new(layer: TileMapLayerKind) -> Self {
        Self { layer, packer: packer::AtlasPacker::new() }
    }
}

impl TextureAtlas for PackedTextureAtlas {
    fn load_texture(&mut self, tex_cache: &mut dyn TextureCache, texture_path: &str) -> TileTexInfo {
        debug_assert!(!texture_path.is_empty());
        if let Some(image) = load_image_file(texture_path) {
            self.packer.pack_image(tex_cache, texture_path, image)
        } else {
            TileTexInfo::default()
        }
    }

    fn commit_textures(&self, tex_cache: &mut dyn TextureCache) {
        log::info!(log::channel!("atlas"), "Committing texture atlas '{}' to graphics memory...", self.layer);

        self.packer.for_each_page(|_, image, texture| {
            let (width, height) = image.dimensions();
            let pixels = image.as_raw();

            tex_cache.update_texture(texture,
                                     0,
                                     0,
                                     Size::new(width as i32, height as i32),
                                     0,
                                     pixels);
        });

        log::info!(log::channel!("atlas"), "Texture atlas '{}' committed.", self.layer);
    }

    fn save_textures_to_file(&self, base_path: &str) {
        let save_path =
            Path::new(base_path)
                .join("atlas")
                .join(self.layer.to_string().to_lowercase());

        let _ = fs::create_dir_all(&save_path);

        log::info!(log::channel!("atlas"),
                   "Saving texture atlas {:?} with {} pages...",
                   save_path,
                   self.packer.page_count());

        self.packer.for_each_page(|index, image, _| {
            let file_path =
                Path::new(&save_path)
                    .join(format!("page_{index}.png"));

            save_image_file(&file_path, image);
        });
    }
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

fn load_image_file<P>(path: P) -> Option<RgbaImage>
    where P: AsRef<Path> + Debug
{
    match ImageReader::open(&path) {
        Ok(reader) => {
            match reader.decode() {
                // Moves data, no pixel conversion if already RGBA8.
                Ok(image) => Some(image.into_rgba8()),
                Err(err) => {
                    log::error!(log::channel!("atlas"), "Failed to decode image file {path:?}: {err:?}");
                    None
                }
            }
        }
        Err(err) => {
            log::error!(log::channel!("atlas"), "Failed to open image file {path:?}: {err:?}");
            None
        }
    }
}

fn save_image_file<P>(path: P, image: &RgbaImage) -> bool
    where P: AsRef<Path> + Debug
{
    let mut file = match fs::File::create(&path) {
        Ok(file) => file,
        Err(err) => {
            log::error!(log::channel!("atlas"), "Failed to create file {path:?}: {err:?}");
            return false;
        }
    };

    if let Err(err) = image.write_to(&mut file, image::ImageFormat::Png) {
        log::error!(log::channel!("atlas"), "Failed to write image file {path:?}: {err:?}");
        return false;
    }

    true
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

    struct AtlasPage {
        packer: TexturePacker<'static, RgbaImage, StringHash>,
        image: RgbaImage,
        texture: TextureHandle,
    }

    pub struct AtlasPacker {
        pages: Vec<AtlasPage>,
    }

    impl AtlasPacker {
        pub fn new() -> Self {
            Self { pages: Vec::new() }
        }

        pub fn pack_image(&mut self, tex_cache: &mut dyn TextureCache, path: &str, image: RgbaImage) -> TileTexInfo {
            debug_assert!(!path.is_empty());
            let key = hash::fnv1a_from_str(path);

            // Image fits an existing page?
            for page in &mut self.pages {
                if !page.packer.can_pack(&image) {
                    continue;
                }

                return Self::do_pack_image(page, key, path, image);
            }

            // Need a new page:
            let page = self.new_page(tex_cache);
            Self::do_pack_image(page, key, path, image)
        }

        pub fn for_each_page<F>(&self, mut visitor_fn: F)
            where F: FnMut(usize, &RgbaImage, TextureHandle)
        {
            for (index, page) in self.pages.iter().enumerate() {
                visitor_fn(index, &page.image, page.texture);
            }
        }

        pub fn page_count(&self) -> usize {
            self.pages.len()
        }

        fn new_page(&mut self, tex_cache: &mut dyn TextureCache) -> &mut AtlasPage {
            let packer = TexturePacker::new_skyline(TEXTURE_PACKER_CONFIG);

            // Allocate a new image/texture with the maximum page dimensions:
            let img_width  = TEXTURE_PACKER_CONFIG.max_width;
            let img_height = TEXTURE_PACKER_CONFIG.max_height;
            let image = RgbaImage::new(img_width, img_height);

            let tex_name = format!("tex_atlas_page_{}", self.page_count());
            let tex_size = Size::new(img_width as i32, img_height as i32);
            let texture = tex_cache.new_uninitialized_texture(&tex_name, tex_size);

            self.pages.push(AtlasPage { packer, image, texture });
            self.pages.last_mut().unwrap()
        }

        fn do_pack_image(page: &mut AtlasPage, key: StringHash, path: &str, image: RgbaImage) -> TileTexInfo {
            if let Err(err) = page.packer.pack_own(key, image) {
                log::error!(log::channel!("atlas"), "Failed to pack texture '{path}' into atlas: {err:?}");
                return TileTexInfo::default();
            }

            // Retrieve the frame metadata:
            let frame = page.packer.get_frame(&key).unwrap();
            let coords = rect_tex_coords_from_frame(frame);
            let texture = page.texture;

            // Update page's image data: blit this sprite image into the page texture.
            let sprite = page.packer.get_texture(&key).unwrap();
            Self::copy_packed_image_to_page(&mut page.image, sprite, frame);

            TileTexInfo { texture, coords }
        }

        fn copy_packed_image_to_page(page: &mut RgbaImage, sprite: &RgbaImage, frame: &Frame<StringHash>) {
            let rect = &frame.frame;
            blit_rgba_image(page, sprite, rect.x, rect.y);
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
    fn blit_rgba_image(dst: &mut RgbaImage, src: &RgbaImage, dst_x: u32, dst_y: u32) {
        let dst_width  = dst.width();
        let dst_height = dst.height();

        let src_width  = src.width();
        let src_height = src.height();

        assert!(dst_x + src_width  <= dst_width);
        assert!(dst_y + src_height <= dst_height);

        let dst_stride = dst_width as usize * 4; // 4 bytes per pixel
        let src_stride = src_width as usize * 4;

        let mut dst_samples = dst.as_flat_samples_mut();
        let src_samples = src.as_flat_samples();

        let dst_buf = dst_samples.as_mut_slice();
        let src_buf = src_samples.as_slice();

        for row in 0..src_height {
            let dst_row_start = ((dst_y + row) as usize * dst_stride) + (dst_x as usize * 4);
            let src_row_start = row as usize * src_stride;
            let dst_row_end   = dst_row_start + src_stride;

            dst_buf[dst_row_start..dst_row_end]
                .copy_from_slice(&src_buf[src_row_start..src_row_start + src_stride]);
        }
    }
}
