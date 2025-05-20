use std::path::Path;
use std::path::PathBuf;

use crate::utils::{Color, Size2D};
use crate::utils::file_sys::{self};
use crate::utils::hash::{self, PreHashedKeyMap, StringHash};
use crate::render::TextureCache;
use super::def::{TileDef, TileKind, TileTexInfo};

// ----------------------------------------------
// Constants
// ----------------------------------------------

pub const PATH_TO_TERRAIN_TILE_SETS:   &str = "assets/tiles/terrain";
pub const PATH_TO_BUILDINGS_TILE_SETS: &str = "assets/tiles/buildings";
pub const PATH_TO_UNITS_TILE_SETS:     &str = "assets/tiles/units";

// ----------------------------------------------
// TileSets
// ----------------------------------------------

pub struct TileSets {
    sets: PreHashedKeyMap<StringHash, TileDef>,
}

impl TileSets {
    pub fn new() -> Self {
        Self {
            sets: PreHashedKeyMap::default()
        }
    }

    pub fn load_all(&mut self, tex_cache: &mut TextureCache) {
        let terrain_tile_sets   = file_sys::collect_sub_dirs(Path::new(PATH_TO_TERRAIN_TILE_SETS));
        let buildings_tile_sets = file_sys::collect_sub_dirs(Path::new(PATH_TO_BUILDINGS_TILE_SETS));
        let units_tile_sets     = file_sys::collect_sub_dirs(Path::new(PATH_TO_UNITS_TILE_SETS));

        println!("Loading Terrain TileSets: {:#?}",   terrain_tile_sets);
        println!("Loading Buildings TileSets: {:#?}", buildings_tile_sets);
        println!("Loading Units TileSets: {:#?}",     units_tile_sets);

        self.load_set(tex_cache, &terrain_tile_sets,   TileKind::Terrain);
        self.load_set(tex_cache, &buildings_tile_sets, TileKind::Building);
        self.load_set(tex_cache, &units_tile_sets,     TileKind::Unit);
    }

    pub fn load_set(&mut self, tex_cache: &mut TextureCache, paths: &Vec<PathBuf>, tile_kind: TileKind) {
        for path in paths {
            let files = file_sys::collect_files(path);

            // Tile name will be the last element of the path:
            //  E.g.: path="assets/tiles/buildings/images/tower" -> tile_name="tower"
            // Each folder contains a set of sprites for the different "frames" of a tile,
            //  E.g.:
            //   tower/b_0.png
            //   tower/b_1.png
            //   ...
            let tile_name = path.file_name().unwrap().to_string_lossy();

            for file in &files {
                // Only look for image files here.
                if file.extension().and_then(|ext| ext.to_str()) != Some("png") {
                    continue;
                }

                let tile_texture_file = file.to_string_lossy();

                println!("Loading tile texture: '{}' [ '{}', {:?} ]",
                         tile_texture_file.as_ref(),
                         tile_name.as_ref(),
                         tile_kind);

                let tile_texture = tex_cache.load_texture(tile_texture_file.as_ref());

                // TODO: Need a metadata file to go with the tile set.
                let tile_def = TileDef{
                    kind: tile_kind,
                    logical_size: Size2D::zero(),
                    draw_size: Size2D::zero(),
                    tex_info: TileTexInfo::new(tile_texture),
                    color: Color::white(),
                    name: tile_name.to_string(),
                };

                self.add_def(tile_def);
            }
        }
    }

    pub fn add_def(&mut self, tile_def: TileDef) {
        let tile_name_hash: StringHash = hash::fnv1a_from_str(&tile_def.name);
        let prev_entry = self.sets.insert(tile_name_hash, tile_def);
        if prev_entry.is_some() {
            panic!("TileSet: An entry for key '{}' ({:#X}) already exists!",
                    prev_entry.unwrap().name,
                    tile_name_hash);
        }
    }

    pub fn find_by_name(&self, name: &str) -> &TileDef {
        let tile_name_hash: StringHash = hash::fnv1a_from_str(name);
        match self.sets.get(&tile_name_hash) {
            Some(tile_def) => tile_def,
            None => TileDef::empty(),
        }
    }

    pub fn defs<'a, F>(&'a self, filter_fn: F) -> impl Iterator<Item = &'a TileDef> + 'a
        where F: Fn(&TileDef) -> bool + 'a {

        self.sets.values().filter(move |v| filter_fn(*v))
    }

    pub fn for_each<F>(&self, visitor_fn: F)
        where F: Fn(StringHash, &TileDef) {

        for (key, tile_def) in &self.sets {
            visitor_fn(*key, tile_def);
        }
    }

    pub fn print(&self, tex_cache: &TextureCache) {
        println!("----- TileSets -----");
        for (key, tile_def) in &self.sets {
            println!("[ key:{:#X}, name:'{}', kind:{:?}, tex:'{}' ]",
                     key,
                     tile_def.name,
                     tile_def.kind,
                     tex_cache.handle_to_texture(tile_def.tex_info.texture).name());
        }
        println!("--------------------");
    }
}
