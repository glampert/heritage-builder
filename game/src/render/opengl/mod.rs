use std::{ffi::CStr, os::raw::c_char};

use crate::log;

pub mod batch;
pub mod buffer;
pub mod context;
pub mod shader;
pub mod system;
pub mod texture;
pub mod vertex;

pub fn log_gl_info() {
    unsafe {
        let gl_version = gl::GetString(gl::VERSION);
        if !gl_version.is_null() {
            log::info!(log::channel!("render"),
                       "GL_VERSION: {}",
                       CStr::from_ptr(gl_version as *const c_char).to_str().unwrap());
        }

        let gl_vendor = gl::GetString(gl::VENDOR);
        if !gl_vendor.is_null() {
            log::info!(log::channel!("render"),
                       "GL_VENDOR: {}",
                       CStr::from_ptr(gl_vendor as *const c_char).to_str().unwrap());
        }

        let glsl_version = gl::GetString(gl::SHADING_LANGUAGE_VERSION);
        if !glsl_version.is_null() {
            log::info!(log::channel!("render"),
                       "GLSL_VERSION: {}",
                       CStr::from_ptr(glsl_version as *const c_char).to_str().unwrap());
        }
    }
}

pub fn gl_error_to_string(error: gl::types::GLenum) -> &'static str {
    match error {
        gl::NO_ERROR => "No error",
        gl::INVALID_ENUM => "Invalid enum",
        gl::INVALID_VALUE => "Invalid value",
        gl::INVALID_OPERATION => "Invalid operation",
        gl::STACK_OVERFLOW => "Stack overflow",
        gl::STACK_UNDERFLOW => "Stack underflow",
        gl::OUT_OF_MEMORY => "Out of memory",
        gl::INVALID_FRAMEBUFFER_OPERATION => "Invalid framebuffer operation",
        _ => "Unknown error",
    }
}

pub fn panic_if_gl_error() {
    let error_code = unsafe { gl::GetError() };
    if error_code != gl::NO_ERROR {
        panic!("OpenGL Error: {} (0x{:X})", gl_error_to_string(error_code), error_code);
    }
}
