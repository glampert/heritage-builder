
// ================================================================================================
// File: texcache.rs
// Author: Guilherme R. Lampert
// Created on: 29/02/16
// Brief: Texture loading and caching.
//
// This source code is released under the MIT license.
// See the accompanying LICENSE file for details.
// ================================================================================================

extern crate xml;
extern crate image;
extern crate glium;

use std;
use std::path::Path;
use std::fs::File;
use std::io::BufReader;
use xml::reader::{EventReader, XmlEvent};

use citysim::common::*;
use citysim::tile::{Tile, TileGeometry};

// ----------------------------------------------
// TextureAtlas
// ----------------------------------------------

pub struct TexAtlasSubTexture {
    pub filename:     String, // Original image that made this sub-texture.
    pub x:            i32,
    pub y:            i32,
    pub width:        i32,
    pub height:       i32,
    pub frame_x:      i32,
    pub frame_y:      i32,
    pub frame_width:  i32,
    pub frame_height: i32,
}

impl TexAtlasSubTexture {
    pub fn new() -> TexAtlasSubTexture {
        TexAtlasSubTexture{
            filename: String::new(),
            x:           0, y:            0,
            width:       0, height:       0,
            frame_x:     0, frame_y:      0,
            frame_width: 0, frame_height: 0,
        }
    }
}

pub struct TextureAtlas {
    tex_filename: String,
    sub_textures: Vec<TexAtlasSubTexture>,
}

impl TextureAtlas {
    pub fn get_sub_texture(&self, index: usize) -> &TexAtlasSubTexture {
        &self.sub_textures[index]
    }

    pub fn get_sub_texture_count(&self) -> i32 {
        self.sub_textures.len() as i32
    }

    pub fn empty() -> TextureAtlas {
        TextureAtlas{ tex_filename: String::new(), sub_textures: Vec::new() }
    }

    pub fn parse_from_xml(xml_filename: &str) -> TextureAtlas {
        let xml_file    = File::open(xml_filename).unwrap();
        let file_reader = BufReader::new(xml_file);
        let xml_parser  = EventReader::new(file_reader);
        let mut atlas   = TextureAtlas{ tex_filename: String::new(), sub_textures: Vec::new() };

        for event in xml_parser {
            match event {
                Ok(XmlEvent::StartElement{ name, attributes, .. }) => {
                    if name.local_name == "SubTexture" {
                        let mut sub_tex = TexAtlasSubTexture::new();
                        for attr in attributes {
                            let attr_name = &attr.name.local_name;
                            match attr_name.as_ref() {
                                "name"        => sub_tex.filename     = attr.value,
                                "x"           => sub_tex.x            = attr.value.parse::<f32>().unwrap() as i32,
                                "y"           => sub_tex.y            = attr.value.parse::<f32>().unwrap() as i32,
                                "width"       => sub_tex.width        = attr.value.parse::<f32>().unwrap() as i32,
                                "height"      => sub_tex.height       = attr.value.parse::<f32>().unwrap() as i32,
                                "frameX"      => sub_tex.frame_x      = attr.value.parse::<f32>().unwrap() as i32,
                                "frameY"      => sub_tex.frame_y      = attr.value.parse::<f32>().unwrap() as i32,
                                "frameWidth"  => sub_tex.frame_width  = attr.value.parse::<f32>().unwrap() as i32,
                                "frameHeight" => sub_tex.frame_height = attr.value.parse::<f32>().unwrap() as i32,
                                _             => {},
                            }
                        }
                        atlas.sub_textures.push(sub_tex);
                    } else if name.local_name == "TextureAtlas" {
                        for attr in attributes {
                            match attr.name.local_name.as_ref() {
                                "imagePath" => atlas.tex_filename = attr.value,
                                _           => {},
                            }
                        }
                    }
                }
                Err(event) => {
                    panic!("parse_tex_atlas() error: {}", event);
                }
                _ => {}
            }
        }

        println!("Finished parsing \"{}\".", xml_filename);
        return atlas;
    }
}

// ----------------------------------------------
// TextureCache
// ----------------------------------------------

pub const TEX_ID_NONE: i32 = -1;
pub type TexId = i32;

pub struct TexCacheEntry {
    pub key:   String,
    pub tex:   glium::texture::SrgbTexture2d,
    pub atlas: TextureAtlas,
}

pub struct TextureCache {
    textures: Vec<TexCacheEntry>,
}

impl TextureCache {
    pub fn new<F>(facade: &F, config: &Config) -> TextureCache
                  where F: glium::backend::Facade {

        let mut tex_cache = TextureCache{ textures: Vec::new() };
        tex_cache.load_all_textures(facade, config);
        return tex_cache;
    }

    pub fn find_by_name(&self, name_key: &String) -> TexId {
        match self.textures.binary_search_by(|probe| probe.key.cmp(name_key)) {
            Err(_)    => TEX_ID_NONE,
            Ok(index) => index as TexId,
        }
    }

    pub fn get_tex_from_id(&self, id: TexId) -> Option<&TexCacheEntry> {
        if id >= 0 && id < self.get_tex_count() {
            Some(&self.textures[id as usize])
        } else {
            None
        }
    }

    pub fn get_tex_count(&self) -> i32 {
        self.textures.len() as i32
    }

    pub fn tile_from_atlas(&self, atlas_tex_id: TexId, tex_num: i32, position: Point2d, color: Color, scale: i32) -> Tile {
        let cache_entry = self.get_tex_from_id(atlas_tex_id).unwrap();
        let sub_tex     = cache_entry.atlas.get_sub_texture(tex_num as usize);

        let inv_width  = 1.0 / (cache_entry.tex.get_width() as f32);
        let inv_height = 1.0 / (cache_entry.tex.get_height().unwrap() as f32);

        let x = (sub_tex.x as f32) * inv_width;
        let y = (sub_tex.y as f32) * inv_width;
        let w = (sub_tex.width  as f32) * inv_width;
        let h = (sub_tex.height as f32) * inv_height;
        let tex_coords = [x, y, x, y + h, x + w, y + h, x + w, y];

        let rect = Rect2d::with_bounds(position.x, position.y,
                                       position.x + sub_tex.width  * scale,
                                       position.y + sub_tex.height * scale);

        Tile{
            tex_id:   atlas_tex_id,
            geometry: TileGeometry{ rect: rect, color: color, tex_coords: tex_coords }
        }
    }

    fn load_all_textures<F>(&mut self, facade: &F, config: &Config)
                            where F: glium::backend::Facade {

        // Preload all the stuff:
        self.load_atlases(facade, config);

        // Keep it sorted for faster binary searches.
        self.textures.sort_by(|a, b| a.key.cmp(&b.key));
        println!("TextureCache loaded!");
    }

    fn load_atlases<F>(&mut self, facade: &F, config: &Config)
                       where F: glium::backend::Facade {

        let path_sep  = std::path::MAIN_SEPARATOR;
        let base_path = TEXTURE_ATLAS_BASE_PATH;
        let meta_ext  = TEXTURE_ATLAS_META_FILE_EXT;
        let tex_ext   = TEXTURE_ATLAS_TEX_FILE_EXT;

        let tex_atlas_list = config.get_texture_atlases();
        for atlas_file in tex_atlas_list {
            let tex_file_path = format!("{}{}{}{}", base_path, path_sep, atlas_file, tex_ext);
            let as_sys_path   = Path::new(&tex_file_path);

            let meta_file_path = format!("{}{}{}{}", base_path, path_sep, atlas_file, meta_ext);
            let atlas = TextureAtlas::parse_from_xml(meta_file_path.as_ref());

            if !self.try_load_texture(facade, as_sys_path, format!("{}", atlas_file), atlas) {
                panic!("Can't load texture atlas \"{}\"!", tex_file_path);
            }
        }
    }

    fn try_load_texture<F>(&mut self, facade: &F, file_path: &Path, name_key: String, atlas: TextureAtlas)
                           -> bool where F: glium::backend::Facade {

        let image = match image::open(file_path) {
            Err(_)    => return false,
            Ok(image) => image.to_rgba(),
        };

        let dims    = image.dimensions();
        let image   = glium::texture::RawImage2d::from_raw_rgba(image.into_raw(), dims);
        let texture = glium::texture::SrgbTexture2d::new(facade, image).unwrap();

        println!("Texture '{}' => \"{}\" ({}x{}) successfully loaded.",
                 name_key, file_path.display(), dims.0, dims.1);

        self.textures.push(TexCacheEntry{ key: name_key, tex: texture, atlas: atlas });
        return true;
    }
}
