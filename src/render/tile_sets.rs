use std::path::Path;
use std::path::PathBuf;
use crate::utils::{Color, Size2D};
use crate::utils::file_sys::{self};
use crate::utils::hash::{self, PreHashedKeyMap, StringHash};
use super::opengl::texture::TextureCache;
use super::tile_def::{TileDef, TileKind, TileTexInfo};

// ----------------------------------------------
// Constants
// ----------------------------------------------

const PATH_TO_TERRAIN_TILE_SETS:   &str = "assets/tiles/terrain";
const PATH_TO_BUILDINGS_TILE_SETS: &str = "assets/tiles/buildings";
const PATH_TO_UNITS_TILE_SETS:     &str = "assets/tiles/units";

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

    pub fn find_by_name(&self, name: &str) -> &TileDef {
        let tile_name_hash: StringHash = hash::fnv1a_from_str(name);
        match self.sets.get(&tile_name_hash) {
            Some(tile_def) => tile_def,
            None => TileDef::empty(),
        }
    }

    pub fn load_all(&mut self, tex_cache: &mut TextureCache) {
        let terrain_tile_sets   = file_sys::collect_sub_dirs(Path::new(PATH_TO_TERRAIN_TILE_SETS));
        let buildings_tile_sets = file_sys::collect_sub_dirs(Path::new(PATH_TO_BUILDINGS_TILE_SETS));
        let units_tile_sets     = file_sys::collect_sub_dirs(Path::new(PATH_TO_UNITS_TILE_SETS));

        println!("Loading Terrain TileSets: {:#?}",   terrain_tile_sets);
        println!("Loading Buildings TileSets: {:#?}", buildings_tile_sets);
        println!("Loading Units TileSets: {:#?}",     units_tile_sets);

        self.load(tex_cache, &terrain_tile_sets,   TileKind::Terrain);
        self.load(tex_cache, &buildings_tile_sets, TileKind::Building);
        self.load(tex_cache, &units_tile_sets,     TileKind::Unit);
    }

    pub fn load(&mut self, tex_cache: &mut TextureCache, paths: &Vec<PathBuf>, tile_kind: TileKind) {
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
                    tex_info: TileTexInfo::with_texture(tile_texture),
                    color: Color::white(),
                    name: tile_name.to_string(),
                };

                let tile_name_hash: StringHash = hash::fnv1a_from_str(&tile_def.name);
                let prev_entry = self.sets.insert(tile_name_hash, tile_def);

                if prev_entry.is_some() {
                    panic!("TileSet: An entry for key '{}' ({:#X}) already exists!",
                           tile_name,
                           tile_name_hash);
                }
            }
        }
    }

    pub fn list_all(&self, tex_cache: &TextureCache) {
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

    pub fn new_with_test_tiles(tex_cache: &mut TextureCache) -> Self {
        println!("Loading test tile sets...");

        let mut tile_sets = TileSets::new();

        let tex_ground = tex_cache.load_texture(&(PATH_TO_TERRAIN_TILE_SETS.to_string() + "/ground/0.png"));
        let tex_house  = tex_cache.load_texture(&(PATH_TO_BUILDINGS_TILE_SETS.to_string() + "/house/0.png"));
        let tex_tower  = tex_cache.load_texture(&(PATH_TO_BUILDINGS_TILE_SETS.to_string() + "/tower/0.png"));
        let tex_ped    = tex_cache.load_texture(&(PATH_TO_UNITS_TILE_SETS.to_string() + "/ped/0.png"));

        let tile_defs: [TileDef; 5] = [
            TileDef { kind: TileKind::Terrain,  logical_size: Size2D{ width: 64,  height: 32 }, draw_size: Size2D{ width: 64,  height: 32  }, tex_info: TileTexInfo::with_texture(tex_ground), color: Color::green(), name: "grass".to_string() },
            TileDef { kind: TileKind::Terrain,  logical_size: Size2D{ width: 64,  height: 32 }, draw_size: Size2D{ width: 64,  height: 32  }, tex_info: TileTexInfo::with_texture(tex_ground), color: Color::white(), name: "road".to_string()  },
            TileDef { kind: TileKind::Building, logical_size: Size2D{ width: 128, height: 64 }, draw_size: Size2D{ width: 128, height: 68  }, tex_info: TileTexInfo::with_texture(tex_house),  color: Color::white(), name: "house".to_string() },
            TileDef { kind: TileKind::Building, logical_size: Size2D{ width: 192, height: 96 }, draw_size: Size2D{ width: 192, height: 144 }, tex_info: TileTexInfo::with_texture(tex_tower),  color: Color::white(), name: "tower".to_string() },
            TileDef { kind: TileKind::Unit,     logical_size: Size2D{ width: 64,  height: 32 }, draw_size: Size2D{ width: 16,  height: 24  }, tex_info: TileTexInfo::with_texture(tex_ped),    color: Color::white(), name: "ped".to_string()   },
        ];

        for tile_def in tile_defs {
            let tile_name_hash: StringHash = hash::fnv1a_from_str(&tile_def.name);
            let prev_entry = tile_sets.sets.insert(tile_name_hash, tile_def);
            assert!(prev_entry.is_none());
        }

        tile_sets
    }
}
