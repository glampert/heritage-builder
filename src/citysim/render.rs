
// ================================================================================================
// File: render.rs
// Author: Guilherme R. Lampert
// Created on: 29/02/16
// Brief: Rendering utilities.
//
// This source code is released under the MIT license.
// See the accompanying LICENSE file for details.
// ================================================================================================

extern crate glium;

use glium::Surface;
use citysim::texcache::TextureCache;
use citysim::common::Config;
use citysim::tile::{Tile, TileGeometry};

// ----------------------------------------------
// DrawIndex / DrawVertex:
// ----------------------------------------------

pub type DrawIndex = u16;

#[derive(Copy, Clone)]
pub struct DrawVertex {
    pub position:   [f32; 2], // X,Y
    pub tex_coords: [f32; 2], // U,V
    pub color:      [f32; 4], // R,G,B,A
}
implement_vertex!(DrawVertex, position, tex_coords, color);

// ----------------------------------------------
// BatchRenderer
// ----------------------------------------------

const BATCH_VB_SIZE: usize = 2048; // Size in DrawVertexs
const BATCH_IB_SIZE: usize = 4096; // Size in DrawIndexes

#[derive(Clone)]
struct BatchBucket {
    geometry: Vec<TileGeometry>,    // tile rectangle, color, UVs, ...
    index_buffer_slice: (u32, u32), // (fist_index, index_count)
}

impl BatchBucket {
    fn new() -> BatchBucket {
        BatchBucket { geometry: Vec::new(), index_buffer_slice: (0,0) }
    }

    fn clear(&mut self) {
        self.geometry.clear();
        self.index_buffer_slice = (0,0);
    }
}

pub struct BatchRenderer {
    texture_buckets: Vec<BatchBucket>,
    shader_prog:     glium::Program,
    vertex_buffer:   glium::VertexBuffer<DrawVertex>,
    index_buffer:    glium::IndexBuffer<DrawIndex>,
    local_verts:     Vec<DrawVertex>,
    local_indexes:   Vec<DrawIndex>,
    tile_count:      u32,
}

impl BatchRenderer {
    pub fn new<F>(facade: &F, config: &Config, tex_cache: &TextureCache) -> BatchRenderer
                  where F: glium::backend::Facade {

        let prim = glium::index::PrimitiveType::TrianglesList;
        let vb   = glium::VertexBuffer::empty_dynamic(facade, BATCH_VB_SIZE).unwrap();
        let ib   = glium::IndexBuffer::empty_dynamic(facade, prim, BATCH_IB_SIZE).unwrap();

        let mut buckets = Vec::new();
        buckets.resize(tex_cache.get_tex_count() as usize, BatchBucket::new());
        println!("BatchRenderer created!");

        BatchRenderer{
            texture_buckets: buckets,
            shader_prog:     BatchRenderer::make_shader_prog(facade, config),
            vertex_buffer:   vb,
            index_buffer:    ib,
            local_verts:     Vec::with_capacity(BATCH_VB_SIZE),
            local_indexes:   Vec::with_capacity(BATCH_IB_SIZE),
            tile_count:      0,
        }
    }

    pub fn add_tile(&mut self, tile: &Tile) {
        let bucket_index = tile.tex_id as usize;
        self.texture_buckets[bucket_index].geometry.push(tile.geometry);
        self.tile_count += 1;
    }

    pub fn clear(&mut self) {
        for bucket in &mut self.texture_buckets {
            bucket.clear();
        }
        self.local_verts.clear();
        self.local_indexes.clear();
        self.tile_count = 0;
    }

    pub fn update(&mut self) {
        let base_indexes = &[0, 1, 2,  2, 3, 0];
        let mut base_vertex = 0;

        // Assemble the quadrilaterals:
        for bucket in &mut self.texture_buckets {
            bucket.index_buffer_slice.0 = self.local_indexes.len() as u32;
            for entry in &mut bucket.geometry {
                let quad = BatchRenderer::make_quad_verts(entry);
                self.local_verts.extend_from_slice(&quad);
                for idx in base_indexes {
                    self.local_indexes.push((idx + base_vertex) as DrawIndex);
                }
                base_vertex += 4;
            }
            bucket.index_buffer_slice.1 = self.local_indexes.len() as u32;
        }

        if self.local_verts.len() > BATCH_VB_SIZE {
            panic!("BATCH_VB_SIZE exceeded!!!");
        }
        if self.local_indexes.len() > BATCH_IB_SIZE {
            panic!("BATCH_IB_SIZE exceeded!!!");
        }

        // Upload to the GL:
        let mut buffer_index = 0;
        let mut vb_mapping = self.vertex_buffer.map_write();
        for v in &self.local_verts {
            vb_mapping.set(buffer_index, v.clone());
            buffer_index += 1;
        }

        buffer_index = 0;
        let mut ib_mapping = self.index_buffer.map_write();
        for i in &self.local_indexes {
            ib_mapping.set(buffer_index, i.clone());
            buffer_index += 1;
        }
    }

    pub fn draw(&self, target: &mut glium::Frame, tex_cache: &TextureCache) {
        if self.tile_count == 0 {
            return; // Nothing to draw.
        }

        let draw_params = glium::DrawParameters{
            blend: glium::Blend::alpha_blending(),
            .. Default::default()
        };

        let screen_dimensions = (target.get_dimensions().0 as f32,
                                 target.get_dimensions().1 as f32);

        // One draw call for each texture:
        let mut tex_id = 0;
        for bucket in &self.texture_buckets {
            let uniforms = uniform!{
                screen_dimensions: screen_dimensions,
                texture_sampler: &tex_cache.get_tex_from_id(tex_id).unwrap().tex,
            };

            let start = bucket.index_buffer_slice.0 as usize;
            let end   = bucket.index_buffer_slice.1 as usize;
            let slice = self.index_buffer.slice(start .. end).unwrap();

            target.draw(&self.vertex_buffer, &slice, &self.shader_prog, &uniforms, &draw_params).unwrap();
            tex_id += 1;
        }
    }

    fn make_quad_verts(geom: &TileGeometry) -> [DrawVertex; 4] {
        let x = geom.rect.x() as f32;
        let y = geom.rect.y() as f32;
        let w = geom.rect.width()  as f32;
        let h = geom.rect.height() as f32;
        let uvs  = &geom.tex_coords;
        let clr  = &geom.color;
        let rgba = [ clr.r, clr.g, clr.b, clr.a ];
        [ DrawVertex{ position: [x,     y    ], tex_coords: [uvs[0], uvs[1]], color: rgba },
          DrawVertex{ position: [x,     y + h], tex_coords: [uvs[2], uvs[3]], color: rgba },
          DrawVertex{ position: [x + w, y + h], tex_coords: [uvs[4], uvs[5]], color: rgba },
          DrawVertex{ position: [x + w, y    ], tex_coords: [uvs[6], uvs[7]], color: rgba } ]
    }

    fn make_shader_prog<F>(facade: &F, config: &Config) -> glium::Program
                           where F: glium::backend::Facade {
        glium::Program::from_source(facade,
                        config.get_tile_draw_vs(),
                        config.get_tile_draw_fs(), None).unwrap()
    }
}
