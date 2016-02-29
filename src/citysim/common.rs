
// ================================================================================================
// File: common.rs
// Author: Guilherme R. Lampert
// Created on: 29/02/16
// Brief: Common code and utilities used by the project.
//
// This source code is released under the MIT license.
// See the accompanying LICENSE file for details.
// ================================================================================================

use std;

// ----------------------------------------------
// Color
// ----------------------------------------------

#[derive(Copy, Clone)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub fn new()   -> Color { Color{ r: 0.0, g: 0.0, b: 0.0, a: 1.0 } }
    pub fn white() -> Color { Color{ r: 1.0, g: 1.0, b: 1.0, a: 1.0 } }
    pub fn black() -> Color { Color{ r: 0.0, g: 0.0, b: 0.0, a: 1.0 } }
    pub fn red()   -> Color { Color{ r: 1.0, g: 0.0, b: 0.0, a: 1.0 } }
    pub fn gree()  -> Color { Color{ r: 0.0, g: 1.0, b: 0.0, a: 1.0 } }
    pub fn blue()  -> Color { Color{ r: 0.0, g: 0.0, b: 1.0, a: 1.0 } }
}

// ----------------------------------------------
// Point2d
// ----------------------------------------------

#[derive(Copy, Clone)]
pub struct Point2d {
    pub x: i32,
    pub y: i32,
}

impl Point2d {
    pub fn new() -> Point2d {
        Point2d{ x: 0, y: 0 }
    }
    pub fn with_coords(cx: i32, cy: i32) -> Point2d {
        Point2d{ x: cx, y: cy }
    }
}

// ----------------------------------------------
// Rect2d
// ----------------------------------------------

#[derive(Copy, Clone)]
pub struct Rect2d {
    pub mins: Point2d,
    pub maxs: Point2d,
}

impl Rect2d {
    pub fn new() -> Rect2d {
        Rect2d{ mins: Point2d::new(), maxs: Point2d::new() }
    }
    pub fn with_bounds(x_min: i32, y_min: i32, x_max: i32, y_max: i32) -> Rect2d {
        Rect2d{ mins: Point2d::with_coords(x_min, y_min), maxs: Point2d::with_coords(x_max, y_max) }
    }

    pub fn x(&self)      -> i32 { self.mins.x }
    pub fn y(&self)      -> i32 { self.mins.y }
    pub fn width(&self)  -> i32 { self.maxs.x - self.mins.x }
    pub fn height(&self) -> i32 { self.maxs.y - self.mins.y }
    pub fn area(&self)   -> i32 { self.width() * self.height() }
}

// ----------------------------------------------
// Config
// ----------------------------------------------

pub struct Config {
    pub version: f32,
}

// We might eventually want to source some
// of the parameters from an external XML file...
impl Config {
    pub fn new() -> Config {
        Config::pwd();
        println!("Initializing runtime configurations...");
        Config{ version: 1.0 }
    }

    pub fn get_initial_screen_dimensions(&self) -> (u32, u32) {
        (1024, 768)
    }
    pub fn get_texture_atlases(&self) -> &'static [&'static str] {
        TEXTURE_ATLASES
    }
    pub fn get_tile_draw_fs(&self) -> &'static str {
        TILE_FRAGMENT_SHADER_SRC
    }
    pub fn get_tile_draw_vs(&self) -> &'static str {
        TILE_VERTEX_SHADER_SRC
    }

    fn pwd() {
        let cwd = std::env::current_dir().unwrap();
        println!("The current directory is \"{}\".", cwd.display());
    }
}

// ----------------------------------------------
// Miscellaneous compile-time constants:
// ----------------------------------------------

pub static TEXTURE_ATLAS_BASE_PATH:     &'static str = "atlases";
pub static TEXTURE_ATLAS_META_FILE_EXT: &'static str = ".xml";
pub static TEXTURE_ATLAS_TEX_FILE_EXT:  &'static str = ".png";

static TEXTURE_ATLASES: &'static [&'static str] = &[
    "house-tileset",
];

// ----------------------------------------------
// Inline GLSL shaders:
// ----------------------------------------------

const TILE_VERTEX_SHADER_SRC: &'static str = r#"
    #version 150

    in vec2 position;
    in vec2 tex_coords;
    in vec4 color;

    out vec2 v_tex_coords;
    out vec4 v_color;

    uniform vec2 screen_dimensions;

    void main() {
        v_tex_coords = tex_coords;
        v_color      = color;

        // Map to normalized clip coordinates:
        // 'position' comes in as screen space.
        float x = ((2.0 * (position.x - 0.5)) / screen_dimensions.x) - 1.0;
        float y = 1.0 - ((2.0 * (position.y - 0.5)) / screen_dimensions.y);
        gl_Position = vec4(x, y, 0.0, 1.0);
    }
"#;

const TILE_FRAGMENT_SHADER_SRC: &'static str = r#"
    #version 150

    in vec2 v_tex_coords;
    in vec4 v_color;
    out vec4 frag_color;

    uniform sampler2D texture_sampler;

    void main() {
        frag_color = texture(texture_sampler, v_tex_coords) * v_color;
    }
"#;
