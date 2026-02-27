use super::{
    panic_if_gl_error,
    buffer::NULL_BUFFER_HANDLE,
};
use crate::{
    log,
    render,
    utils::Size,
};

// ----------------------------------------------
// RenderTarget
// ----------------------------------------------

pub struct RenderTarget {
    size: Size,
    framebuffer_handle: gl::types::GLuint,
    depth_buffer_handle: gl::types::GLuint,  // Optional depth buffer.
    color_rt_texture: render::TextureHandle, // Mandatory color render target texture.
    blit_filter: gl::types::GLenum,          // Filter used when blitting the color rt to screen.
}

impl RenderTarget {
    pub fn new(tex_cache: &mut impl render::TextureCache,
               size: Size,
               with_depth_buffer: bool,
               sampling_filter: render::TextureFilter,
               debug_name: &str) -> Self
    {
        debug_assert!(size.is_valid());

        let color_rt_texture = tex_cache.new_uninitialized_texture(
            debug_name,
            size,
            Some(render::TextureSettings {
                filter: sampling_filter,
                wrap_mode: render::TextureWrapMode::ClampToEdge,
                gen_mipmaps: false,
            }));

        let (framebuffer_handle, depth_buffer_handle) = unsafe {
            let mut framebuffer_handle  = NULL_BUFFER_HANDLE;
            let mut depth_buffer_handle = NULL_BUFFER_HANDLE;

            gl::GenFramebuffers(1, &mut framebuffer_handle);
            if framebuffer_handle == NULL_BUFFER_HANDLE {
                panic!("Failed to create framebuffer handle!");
            }

            gl::BindFramebuffer(gl::FRAMEBUFFER, framebuffer_handle);

            let color_rt_tex2d = tex_cache.to_native_handle(color_rt_texture);
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                color_rt_tex2d.bits as _,
                0,
            );

            // Optional Depth Buffer:
            if with_depth_buffer {
                gl::GenRenderbuffers(1, &mut depth_buffer_handle);
                if depth_buffer_handle == NULL_BUFFER_HANDLE {
                    panic!("Failed to create depth buffer handle!");
                }

                gl::BindRenderbuffer(gl::RENDERBUFFER, depth_buffer_handle);

                gl::RenderbufferStorage(
                    gl::RENDERBUFFER,
                    gl::DEPTH24_STENCIL8,
                    size.width,
                    size.height
                );

                gl::FramebufferRenderbuffer(
                    gl::FRAMEBUFFER,
                    gl::DEPTH_STENCIL_ATTACHMENT,
                    gl::RENDERBUFFER,
                    depth_buffer_handle
                );

                gl::BindRenderbuffer(gl::RENDERBUFFER, NULL_BUFFER_HANDLE);
            }

            let framebuffer_status = gl::CheckFramebufferStatus(gl::FRAMEBUFFER);
            if framebuffer_status != gl::FRAMEBUFFER_COMPLETE {
                log::error!(log::channel!("render"), "Invalid framebuffer status for '{debug_name}': 0x{:X}", framebuffer_status);
            }

            panic_if_gl_error();

            gl::BindFramebuffer(gl::FRAMEBUFFER, NULL_BUFFER_HANDLE);

            (framebuffer_handle, depth_buffer_handle)
        };

        let blit_filter = match sampling_filter {
            render::TextureFilter::Nearest |
            render::TextureFilter::NearestMipmapNearest |
            render::TextureFilter::NearestMipmapLinear => {
                gl::NEAREST
            }
            render::TextureFilter::Linear |
            render::TextureFilter::LinearMipmapNearest |
            render::TextureFilter::LinearMipmapLinear => {
                gl::LINEAR
            }
        };

        Self {
            size,
            framebuffer_handle,
            depth_buffer_handle,
            color_rt_texture,
            blit_filter,
        }
    }

    pub fn blit_to_screen(&self, dest_size: Size) {
        debug_assert!(dest_size.is_valid());
        debug_assert!(self.is_valid() && self.blit_filter != 0);

        unsafe {
            // Bind source framebuffer for reading:
            gl::BindFramebuffer(gl::READ_FRAMEBUFFER, self.framebuffer_handle);

            // Bind default framebuffer for drawing:
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, NULL_BUFFER_HANDLE);

            // Blit color buffer:
            gl::BlitFramebuffer(
                0,
                0,
                self.size.width,
                self.size.height,
                0,
                0,
                dest_size.width,
                dest_size.height,
                gl::COLOR_BUFFER_BIT,
                self.blit_filter
            );

            gl::BindFramebuffer(gl::READ_FRAMEBUFFER, NULL_BUFFER_HANDLE);
        }
    }

    pub fn is_valid(&self) -> bool {
        self.framebuffer_handle != NULL_BUFFER_HANDLE &&
        self.color_rt_texture.is_valid() &&
        self.size.is_valid()
    }

    pub fn handle(&self) -> gl::types::GLuint {
        self.framebuffer_handle
    }

    pub fn size(&self) -> Size {
        self.size
    }

    pub fn has_depth_buffer(&self) -> bool {
        self.depth_buffer_handle != NULL_BUFFER_HANDLE
    }

    pub fn color_texture_handle(&self) -> render::TextureHandle {
        self.color_rt_texture
    }
}
