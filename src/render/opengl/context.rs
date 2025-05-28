use std::ffi::c_void;

use crate::{
    utils::{Color, Rect2D}
};

use super::{
    {panic_if_gl_error, log_gl_info},
    shader::{ShaderProgram, NULL_SHADER_HANDLE},
    texture::{Texture2D, TextureUnit, MAX_TEXTURE_UNITS, NULL_TEXTURE_HANDLE},
    buffer::{IndexType, VertexArray, NULL_VERTEX_ARRAY_HANDLE},
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

#[repr(u32)]
#[derive(Copy, Clone)]
pub enum PrimitiveTopology {
    Points = gl::POINTS,
    Lines = gl::LINES,
    Triangles = gl::TRIANGLES,
}

#[repr(u32)]
#[derive(Copy, Clone)]
pub enum AlphaBlend {
    Enabled,
    Disabled,
}

#[repr(u32)]
#[derive(Copy, Clone)]
pub enum DepthTest {
    Enabled,
    Disabled,
}

#[repr(u32)]
#[derive(Copy, Clone)]
pub enum BackFaceCulling {
    Enabled,
    Disabled,
}

#[repr(u32)]
#[derive(Copy, Clone)]
pub enum FrontFacing {
    CW,
    CCW,
}

pub struct RenderContext {
    clear_color: Color,
    primitive_topology: PrimitiveTopology,
    current_shader_program: gl::types::GLuint,
    current_vertex_array: gl::types::GLuint,
    current_index_type: Option<IndexType>,
    current_texture2d: [gl::types::GLuint; MAX_TEXTURE_UNITS],
    texture_changes_count: u32,
}

impl RenderContext {
    pub fn new() -> Self {
        log_gl_info();
        Self::enable_program_point_size();

        Self {
            clear_color: Color::black(),
            primitive_topology: PrimitiveTopology::Triangles,
            current_shader_program: NULL_SHADER_HANDLE,
            current_vertex_array: NULL_VERTEX_ARRAY_HANDLE,
            current_index_type: None,
            current_texture2d: [0; MAX_TEXTURE_UNITS],
            texture_changes_count: 0,
        }
    }

    fn enable_program_point_size() {
        unsafe { gl::Enable(gl::PROGRAM_POINT_SIZE); }
    }

    pub fn set_clear_color(&mut self, color: Color) -> &mut Self {
        self.clear_color = color;
        self
    }

    pub fn set_primitive_topology(&mut self, primitive_topology: PrimitiveTopology) -> &mut Self {
        self.primitive_topology = primitive_topology;
        self
    }

    pub fn set_alpha_blend(&mut self, alpha_blend: AlphaBlend) -> &mut Self {
        match alpha_blend {
            AlphaBlend::Enabled => unsafe {
                gl::Enable(gl::BLEND);
                gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
            },
            AlphaBlend::Disabled => unsafe {
                gl::Disable(gl::BLEND);
            },
        }
        self
    }

    pub fn set_depth_test(&mut self, depth_test: DepthTest) -> &mut Self {
        match depth_test {
            DepthTest::Enabled => unsafe {
                gl::Enable(gl::DEPTH_TEST);
                gl::DepthFunc(gl::LESS); // The default.
            },
            DepthTest::Disabled => unsafe {
                gl::Disable(gl::DEPTH_TEST);
            },
        }
        self
    }

    pub fn set_backface_culling(&mut self, backface_culling: BackFaceCulling) -> &mut Self {
        match backface_culling {
            BackFaceCulling::Enabled => unsafe {
                gl::Enable(gl::CULL_FACE);
                gl::CullFace(gl::BACK);
            },
            BackFaceCulling::Disabled => unsafe {
                gl::Disable(gl::CULL_FACE);
            },
        }
        self
    }

    pub fn set_front_facing(&mut self, front_facing: FrontFacing) -> &mut Self {
        match front_facing {
            FrontFacing::CW => unsafe {
                gl::FrontFace(gl::CW);
            },
            FrontFacing::CCW => unsafe {
                gl::FrontFace(gl::CCW);
            },
        }
        self
    }

    pub fn set_viewport(&mut self, rect: Rect2D) -> &mut Self {
        unsafe {
            gl::Viewport(
                rect.x(),
                rect.y(),
                rect.width(),
                rect.height());
        }
        self
    }

    pub fn set_scissor(&mut self, rect: Rect2D) -> &mut Self {
        unsafe {
            gl::Scissor(
                rect.x(),
                rect.y(),
                rect.width(),
                rect.height());
        }
        self
    }

    pub fn set_texture_2d(&mut self, texture: &Texture2D) -> &mut Self {
        debug_assert!(texture.is_valid());

        let tex_unit = texture.tex_unit().0 as usize;
        let tex_handle = texture.handle();

        if self.current_texture2d[tex_unit] != tex_handle {
            self.current_texture2d[tex_unit] = tex_handle;
            self.texture_changes_count += 1;

            unsafe {
                gl::ActiveTexture(gl::TEXTURE0 + (tex_unit as gl::types::GLenum));
                gl::BindTexture(gl::TEXTURE_2D, tex_handle);
            }
        }
        self
    }

    pub fn unset_texture_2d(&mut self, tex_unit: TextureUnit) {
        let tex_unit = tex_unit.0 as usize;
        unsafe {
            gl::ActiveTexture(gl::TEXTURE0 + (tex_unit as gl::types::GLenum));
            gl::BindTexture(gl::TEXTURE_2D, NULL_TEXTURE_HANDLE);
        }
        self.current_texture2d[tex_unit] = NULL_TEXTURE_HANDLE;
    }

    pub fn set_shader_program(&mut self, shader_program: &ShaderProgram) -> &mut Self {
        debug_assert!(shader_program.is_valid());

        let shader_program_handle = shader_program.handle();

        if self.current_shader_program != shader_program_handle {
            self.current_shader_program = shader_program_handle;

            unsafe {
                gl::UseProgram(self.current_shader_program);
            }
        }
        self
    }

    pub fn unset_shader_program(&mut self) {
        unsafe {
            gl::UseProgram(NULL_SHADER_HANDLE);
        }
        self.current_shader_program = NULL_SHADER_HANDLE;
    }

    pub fn set_vertex_array(&mut self, vertex_array: &VertexArray) -> &mut Self {
        debug_assert!(vertex_array.is_valid());

        let vertex_array_handle = vertex_array.handle();

        if self.current_vertex_array != vertex_array_handle {
            self.current_vertex_array = vertex_array_handle;
            self.current_index_type = Some(vertex_array.index_type());

            unsafe {
                gl::BindVertexArray(self.current_vertex_array);
            }
        }
        self
    }

    pub fn unset_vertex_array(&mut self) {
        unsafe {
            gl::BindVertexArray(NULL_VERTEX_ARRAY_HANDLE);
        }
        self.current_vertex_array = NULL_VERTEX_ARRAY_HANDLE;
    }

    pub fn draw(&mut self, first_vertex: u32, vertex_count: u32) {
        debug_assert!(vertex_count != 0);
        debug_assert!(self.current_shader_program != NULL_SHADER_HANDLE);
        debug_assert!(self.current_vertex_array != NULL_VERTEX_ARRAY_HANDLE);

        unsafe {
            gl::DrawArrays(
                self.primitive_topology as gl::types::GLenum,
                first_vertex as gl::types::GLint,
                vertex_count as gl::types::GLint);
        }
    }

    pub fn draw_indexed(&mut self, first_index: u32, index_count: u32) {
        debug_assert!(index_count != 0);
        debug_assert!(self.current_shader_program != NULL_SHADER_HANDLE);
        debug_assert!(self.current_vertex_array != NULL_VERTEX_ARRAY_HANDLE);
        debug_assert!(self.current_index_type.is_some());

        let index_type = self.current_index_type.unwrap();
        let index_type_size_in_bytes = index_type.size_in_bytes();
        let gl_index_type = index_type.to_gl_enum();
        let offset_in_bytes: usize = (first_index as usize) * index_type_size_in_bytes;

        unsafe {
            gl::DrawElements(
                self.primitive_topology as gl::types::GLenum,
                index_count as gl::types::GLsizei,
                gl_index_type,
                offset_in_bytes as *const c_void);
        }
    }

    // Sets and draw the whole VertexArray.
    pub fn draw_vertex_array(&mut self, vertex_array: &VertexArray) {
        self.set_vertex_array(vertex_array);
        self.draw_indexed(0, vertex_array.index_count());
        self.unset_vertex_array();
    }

    pub fn begin_frame(&mut self) {
        self.texture_changes_count = 0;

        unsafe {
            gl::ClearColor(self.clear_color.r,
                         self.clear_color.g,
                          self.clear_color.b,
                         self.clear_color.a);

            gl::Clear(gl::COLOR_BUFFER_BIT);
        }
    }

    pub fn end_frame(&mut self) {
        unsafe {
            // Clear transient GL states.
            gl::UseProgram(NULL_SHADER_HANDLE);
            gl::BindVertexArray(NULL_VERTEX_ARRAY_HANDLE);

            for i in 0..MAX_TEXTURE_UNITS {
                gl::ActiveTexture(gl::TEXTURE0 + (i as gl::types::GLenum));
                gl::BindTexture(gl::TEXTURE_2D, NULL_TEXTURE_HANDLE);
            }
        }

        panic_if_gl_error();

        self.current_shader_program = NULL_SHADER_HANDLE;
        self.current_vertex_array = NULL_VERTEX_ARRAY_HANDLE;
        self.current_index_type = None;
        self.current_texture2d = [0; MAX_TEXTURE_UNITS];
    }

    pub fn texture_changes(&self) -> u32 {
        self.texture_changes_count
    } 
}
