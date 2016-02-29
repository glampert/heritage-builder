
// ================================================================================================
// File: main.rs
// Author: Guilherme R. Lampert
// Created on: 29/02/16
// Brief: Application entry point for the citysim demo.
//
// This source code is released under the MIT license.
// See the accompanying LICENSE file for details.
// ================================================================================================

// http://tomaka.github.io/glium/glium/index.html
// http://tomaka.github.io/glium/book/tuto-01-getting-started.html

#![allow(dead_code)]

#[macro_use]
extern crate glium;
extern crate image;
extern crate xml;

mod citysim;
use citysim::common::*;
use citysim::render::*;
use citysim::texcache::*;

use glium::{DisplayBuild, Surface};

fn main() {
    let config = Config::new();

    let display = glium::glutin::WindowBuilder::new()
        .with_dimensions(config.get_initial_screen_dimensions().0, config.get_initial_screen_dimensions().1)
        .with_title(format!("Hello world"))
        .build_glium()
        .unwrap();

    let tex_cache = TextureCache::new(&display, &config);
    let mut batch = BatchRenderer::new(&display, &config, &tex_cache);

    let tiles_x = 4;
    let tiles_y = 8;
    let tile_width  = 256*2;
    let tile_height = 180*2;

    let mut tex_id = 0;
    let mut x_offset: i32;
    let mut y_offset: i32;

    // TODO: figure out how to generalize these offsets!
    for y in 0..tiles_y {
        // Even row?
        if (y % 2) == 0 {
            x_offset = 0;
            y_offset = y * 232;
        } else {
            x_offset = 252;
            y_offset = y * 232; // extra 2px needed to align perfectly (230=>232)
        }

        for x in 0..tiles_x {
            let tx = (x * tile_width)  + x_offset;
            let ty = (y * tile_height) - y_offset;
            let tile = tex_cache.tile_from_atlas(0, tex_id, Point2d::with_coords(tx, ty), Color::white(), 2);
            batch.add_tile(&tile);
        }

        tex_id = (tex_id + 1) % 4;
    }

    batch.update();

    loop {
        let mut target = display.draw();

        target.clear_color(0.1, 0.1, 0.1, 1.0);

        //batch.update();
        batch.draw(&mut target, &tex_cache);
        //batch.clear();

        target.finish().unwrap();

        assert_no_gl_error!(display);

        for ev in display.poll_events() {
            match ev {
                glium::glutin::Event::Closed => return,
                _ => ()
            }
        }
    }
}

