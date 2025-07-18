use std::ptr;
use std::ffi::CString;
use paste::paste;

use crate::{
    utils::{Vec2, Color}
};

use super::{
    texture::Texture2D
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

pub const NULL_SHADER_HANDLE: gl::types::GLuint = 0;

// ----------------------------------------------
// ShaderVariable
// ----------------------------------------------

pub struct ShaderVariable {
    pub location: gl::types::GLint,
    pub program_handle: gl::types::GLuint, // ShaderProgram it belongs to.
    pub name: String,
}

impl ShaderVariable {
    pub fn is_valid(&self) -> bool {
        // For shader uniform variables the handle can be zero.
        // Only negative values are invalid.
        self.location >= 0 && self.program_handle != NULL_SHADER_HANDLE
    }
}

// ----------------------------------------------
// ShaderVarSetter
// ----------------------------------------------

pub trait ShaderVarTrait {
    fn set_uniform(variable: &ShaderVariable, value: Self);
}

impl ShaderVarTrait for i32 {
    fn set_uniform(variable: &ShaderVariable, value: Self) {
        unsafe {
            gl::ProgramUniform1i(variable.program_handle, variable.location, value);
        }
    }
}

impl ShaderVarTrait for u32 {
    fn set_uniform(variable: &ShaderVariable, value: Self) {
        unsafe {
            gl::ProgramUniform1ui(variable.program_handle, variable.location, value);
        }
    }
}

impl ShaderVarTrait for f32 {
    fn set_uniform(variable: &ShaderVariable, value: Self) {
        unsafe {
            gl::ProgramUniform1f(variable.program_handle, variable.location, value);
        }
    }
}

impl ShaderVarTrait for Vec2 {
    fn set_uniform(variable: &ShaderVariable, value: Self) {
        unsafe {
            gl::ProgramUniform2f(
                variable.program_handle,
                variable.location,
                value.x,
                value.y);
        }
    }
}

impl ShaderVarTrait for Color {
    fn set_uniform(variable: &ShaderVariable, value: Self) {
        unsafe {
            gl::ProgramUniform4f(
                variable.program_handle,
                variable.location,
                value.r,
                value.g,
                value.b,
                value.a);
        }
    }
}

impl ShaderVarTrait for &[f32] {
    fn set_uniform(variable: &ShaderVariable, value: Self) {
        unsafe {
            gl::ProgramUniform1fv(
                variable.program_handle,
                variable.location,
                value.len() as gl::types::GLsizei,
                value.as_ptr());
        }
    }
}

// ----------------------------------------------
// ShaderProgram
// ----------------------------------------------

pub struct ShaderProgram {
    vertex_shader_handle: gl::types::GLuint,
    fragment_shader_handle: gl::types::GLuint,
    program_handle: gl::types::GLuint,
}

impl ShaderProgram {
    pub fn with_vs_code(vertex_shader_code: &str) -> Result<Self, String> {
        let vertex_shader_handle = Self::create_shader(gl::VERTEX_SHADER, vertex_shader_code)?;
        let program_handle = Self::create_program(vertex_shader_handle, NULL_SHADER_HANDLE)?;
        Ok(Self {
            vertex_shader_handle: vertex_shader_handle,
            fragment_shader_handle: NULL_SHADER_HANDLE,
            program_handle: program_handle,
        })
    }

    pub fn with_fs_code(fragment_shader_code: &str) -> Result<Self, String> {
        let fragment_shader_handle = Self::create_shader(gl::FRAGMENT_SHADER, fragment_shader_code)?;
        let program_handle = Self::create_program(NULL_SHADER_HANDLE, fragment_shader_handle)?;
        Ok(Self {
            vertex_shader_handle: NULL_SHADER_HANDLE,
            fragment_shader_handle: fragment_shader_handle,
            program_handle: program_handle,
        })
    }

    pub fn with_vs_fs_code(vertex_shader_code: &str, fragment_shader_code: &str) -> Result<Self, String> {
        let vertex_shader_handle = Self::create_shader(gl::VERTEX_SHADER, vertex_shader_code)?;
        let fragment_shader_handle = Self::create_shader(gl::FRAGMENT_SHADER, fragment_shader_code)?;
        let program_handle = Self::create_program(vertex_shader_handle, fragment_shader_handle)?;
        Ok(Self {
            vertex_shader_handle: vertex_shader_handle,
            fragment_shader_handle: fragment_shader_handle,
            program_handle: program_handle,
        })
    }

    pub fn is_valid(&self) -> bool {
        self.program_handle != NULL_SHADER_HANDLE
    }

    pub fn handle(&self) -> gl::types::GLuint {
        self.program_handle
    }

    // Find variable or panic with error if not found.
    // To fail gracefully, use try_find_variable() instead.
    pub fn find_variable(&self, name: &str) -> ShaderVariable {
        match self.try_find_variable(name) {
            Ok(shader_variable) => shader_variable,
            Err(info_log) => panic!("Shader Var Missing: {}", info_log),
        }
    }

    pub fn try_find_variable(&self, name: &str) -> Result<ShaderVariable, String> {
        debug_assert!(name.is_empty() == false);
        debug_assert!(self.is_valid());

        // Marshall to null terminated C string:
        let c_str_name = CString::new(name.as_bytes()).unwrap();
        let location = unsafe { gl::GetUniformLocation(self.program_handle, c_str_name.as_ptr()) };

        if location < 0 {
            Err(format!("Cannot find shader variable '{}'", name))
        } else {
            Ok(ShaderVariable {
                location: location,
                program_handle: self.program_handle,
                name: name.to_string(),
            })
        }
    }

    pub fn set_variable<T: ShaderVarTrait>(&self, variable: &ShaderVariable, value: T) -> &Self {
        debug_assert!(variable.is_valid());

        if self.program_handle != variable.program_handle {
            panic!("Shader variable '{}' does not belong to this ShaderProgram!", variable.name);
        }

        T::set_uniform(variable, value);
        self
    }

    fn create_shader(shader_type: gl::types::GLenum,
                     shader_code: &str) -> Result<gl::types::GLuint, String> {

        debug_assert!(shader_code.is_empty() == false);

        unsafe {
            let shader_handle = gl::CreateShader(shader_type);
            if shader_handle == NULL_SHADER_HANDLE {
                return Err("gl::CreateShader() failed".to_string());
            }

            // Marshall to null terminated C string:
            let c_str_code = CString::new(shader_code.as_bytes()).unwrap();

            gl::ShaderSource(shader_handle, 1, &c_str_code.as_ptr(), ptr::null());
            gl::CompileShader(shader_handle);

            // Check for shader compile errors:
            const INFO_LOG_BUFFER_SIZE: usize = 512;
            let mut success = gl::FALSE as gl::types::GLint;
            let mut info_log = Vec::with_capacity(INFO_LOG_BUFFER_SIZE);
            info_log.set_len(INFO_LOG_BUFFER_SIZE - 1); // subtract 1 to skip the trailing null character

            gl::GetShaderiv(shader_handle, gl::COMPILE_STATUS, &mut success);

            if success != gl::TRUE as gl::types::GLint {
                gl::GetShaderInfoLog(
                    shader_handle,
                    INFO_LOG_BUFFER_SIZE as gl::types::GLsizei,
                    ptr::null_mut(),
                    info_log.as_mut_ptr() as *mut gl::types::GLchar);

                let shader_stage_prefix = match shader_type {
                    gl::VERTEX_SHADER => "[VS]: ",
                    gl::FRAGMENT_SHADER => "[FS]: ",
                    _ => panic!("Unhandled shader type!"),
                }.to_string();

                let log_string = String::from_utf8(info_log).unwrap();
                return Err(shader_stage_prefix + &log_string);
            }

            Ok(shader_handle)
        }
    }

    fn create_program(vertex_shader_handle: gl::types::GLuint,
                      fragment_shader_handle: gl::types::GLuint) -> Result<gl::types::GLuint, String> {

        // Should have at least one of the two shader stages.
        debug_assert!(vertex_shader_handle != NULL_SHADER_HANDLE ||
                      fragment_shader_handle != NULL_SHADER_HANDLE);

        unsafe {
            let program_handle = gl::CreateProgram();
            if program_handle == NULL_SHADER_HANDLE {
                return Err("gl::CreateProgram() failed".to_string());
            }

            if vertex_shader_handle != NULL_SHADER_HANDLE {
                gl::AttachShader(program_handle, vertex_shader_handle);
            }

            if fragment_shader_handle != NULL_SHADER_HANDLE {
                gl::AttachShader(program_handle, fragment_shader_handle);
            }

            gl::LinkProgram(program_handle);

            // Check for linking errors:
            const INFO_LOG_BUFFER_SIZE: usize = 512;
            let mut success = gl::FALSE as gl::types::GLint;
            let mut info_log = Vec::with_capacity(INFO_LOG_BUFFER_SIZE);
            info_log.set_len(INFO_LOG_BUFFER_SIZE - 1); // subtract 1 to skip the trailing null character
            gl::GetProgramiv(program_handle, gl::LINK_STATUS, &mut success);

            if success != gl::TRUE as gl::types::GLint {
                gl::GetProgramInfoLog(
                    program_handle,
                    INFO_LOG_BUFFER_SIZE as gl::types::GLsizei,
                    ptr::null_mut(),
                    info_log.as_mut_ptr() as *mut gl::types::GLchar);

                let log_string = String::from_utf8(info_log).unwrap();
                return Err(log_string);
            }

            // Bind the program to force OpenGL to fully initialize it now,
            // in case the driver is deferring initialization to first use.
            gl::UseProgram(program_handle);
            gl::UseProgram(NULL_SHADER_HANDLE);

            Ok(program_handle)
        }
    }
}

impl Drop for ShaderProgram {
    fn drop(&mut self) {
        if self.vertex_shader_handle != NULL_SHADER_HANDLE {
            unsafe {
                gl::DeleteShader(self.vertex_shader_handle);
            }
            self.vertex_shader_handle = NULL_SHADER_HANDLE;
        }
        if self.fragment_shader_handle != NULL_SHADER_HANDLE {
            unsafe {
                gl::DeleteShader(self.fragment_shader_handle);
            }
            self.fragment_shader_handle = NULL_SHADER_HANDLE;
        }
        if self.program_handle != NULL_SHADER_HANDLE {
            unsafe {
                gl::DeleteProgram(self.program_handle);
            }
            self.program_handle = NULL_SHADER_HANDLE;
        }
    }
}

// ----------------------------------------------
// Helper functions & macros
// ----------------------------------------------

pub fn new_program_from_code(vs_code: &str, fs_code: &str) -> ShaderProgram {
    match ShaderProgram::with_vs_fs_code(vs_code, fs_code) {
        Ok(shader_program) => shader_program,
        Err(info_log) => panic!("Shader Compilation Error: {}", info_log),
    }
}

pub fn set_variable_by_name<T: ShaderVarTrait>(shader_program: &ShaderProgram, var_name: &str, value: T) {
    let shader_var = shader_program.find_variable(var_name);
    shader_program.set_variable(&shader_var, value);
}

#[macro_export]
macro_rules! shader {
    (
        $mod_name:ident,
        $($field:ident : $field_type:ty),* $(,)?
    ) => {
        pub mod $mod_name {
            use super::*;

            pub struct Vars {
                $(
                    pub $field: ShaderVariable,
                )*
            }

            pub struct Shader {
                pub variables: Vars,
                pub program: ShaderProgram,
            }

            impl Shader {
                pub fn load() -> Self {
                    const VS_SRC: &str = include_str!(
                        concat!("shaders/", stringify!($mod_name), ".vert")
                    );
                    const FS_SRC: &str = include_str!(
                        concat!("shaders/", stringify!($mod_name), ".frag")
                    );

                    let program = new_program_from_code(VS_SRC, FS_SRC);
                    Self {
                        variables: Vars {
                            $(
                                $field: program.find_variable($crate::name_of!(Vars, $field)),
                            )*
                        },
                        program,
                    }
                }
                // Generate strongly-typed setters for each shader uniform variable.
                // This uses the `paste` Rust crate to generate each set_varname() method.
                paste! {
                    $(
                        pub fn [<set_ $field>](&self, value: $field_type) {
                            self.program.set_variable(&self.variables.$field, value);
                        }
                    )*
                }
            }
        }
    };
}

// ----------------------------------------------
// Built-in shaders
// ----------------------------------------------

shader!(
    sprites,
    // Uniform variables:
    viewport_size : Vec2,
    sprite_tint : Color,
    sprite_texture : &Texture2D,
);

shader!(
    lines,
    // Uniform variables:
    viewport_size : Vec2,
);

shader!(
    points,
    // Uniform variables:
    viewport_size : Vec2,
);
