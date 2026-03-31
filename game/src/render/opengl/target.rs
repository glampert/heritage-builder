use super::{
    panic_if_gl_error,
    buffer::NULL_BUFFER_HANDLE,
    texture::{
        TextureSettings, TextureFilter, TextureWrapMode,
        TextureUnit, TextureCreationParams, OpenGlTexture,
    },
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
    color_rt_texture: OpenGlTexture, // Mandatory color render target texture.
    blit_filter: gl::types::GLenum,  // Filter used when blitting the color rt to screen.
}

impl RenderTarget {
    pub fn new(size: Size, sampling_filter: TextureFilter) -> Self {
        debug_assert!(size.is_valid());

        let color_rt_texture = OpenGlTexture::new(
            TextureCreationParams {
                name: "offscreen_render_target",
                size,
                pixels: &[],
                settings: TextureSettings {
                    filter: sampling_filter,
                    wrap_mode: TextureWrapMode::ClampToEdge,
                    mipmaps: false,
                },
                tex_unit: TextureUnit(0),
                allow_settings_change: false,
            }
        );

        let framebuffer_handle = unsafe {
            let mut framebuffer_handle = NULL_BUFFER_HANDLE;

            gl::GenFramebuffers(1, &mut framebuffer_handle);
            if framebuffer_handle == NULL_BUFFER_HANDLE {
                panic!("Failed to create framebuffer handle!");
            }

            gl::BindFramebuffer(gl::FRAMEBUFFER, framebuffer_handle);

            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                color_rt_texture.handle(),
                0,
            );

            let framebuffer_status = gl::CheckFramebufferStatus(gl::FRAMEBUFFER);
            if framebuffer_status != gl::FRAMEBUFFER_COMPLETE {
                log::error!(log::channel!("render"),
                            "Invalid framebuffer status for 'offscreen_render_target': 0x{:X}",
                            framebuffer_status);
            }

            panic_if_gl_error();

            gl::BindFramebuffer(gl::FRAMEBUFFER, NULL_BUFFER_HANDLE);

            framebuffer_handle
        };

        let blit_filter = match sampling_filter {
            TextureFilter::Nearest |
            TextureFilter::NearestMipmapNearest |
            TextureFilter::NearestMipmapLinear => {
                gl::NEAREST
            }
            TextureFilter::Linear |
            TextureFilter::LinearMipmapNearest |
            TextureFilter::LinearMipmapLinear => {
                gl::LINEAR
            }
        };

        Self {
            size,
            framebuffer_handle,
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

    #[inline]
    pub fn is_valid(&self) -> bool {
        use render::texture::Texture;

        self.framebuffer_handle != NULL_BUFFER_HANDLE
            && self.color_rt_texture.is_valid()
            && self.size.is_valid()
    }

    #[inline]
    pub fn handle(&self) -> gl::types::GLuint {
        self.framebuffer_handle
    }

    #[inline]
    pub fn size(&self) -> Size {
        self.size
    }
}

impl Drop for RenderTarget {
    fn drop(&mut self) {
        if self.framebuffer_handle != NULL_BUFFER_HANDLE {
            unsafe {
                gl::DeleteFramebuffers(1, &self.framebuffer_handle);
            }

            self.framebuffer_handle = NULL_BUFFER_HANDLE;
        }
    }
}
