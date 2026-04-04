use super::UiTextureHandle;
use common::{Rect, Vec2, Size, mem::RcMut};
use crate::render::{
    RenderSystem,
    texture::{TextureHandle, TextureWrapMode, TextureFilter, TextureSettings},
};

pub type UiDrawIndex  = imgui::DrawIdx;
pub type UiDrawVertex = imgui::DrawVert;

// ----------------------------------------------
// UiDrawArgs
// ----------------------------------------------

struct UiDrawArgs {
    scale_w: f32,
    scale_h: f32,
    fb_width: f32,
    fb_height: f32,
    count: usize,
    cmd_params: imgui::DrawCmdParams,
}

// ----------------------------------------------
// UiRenderFrameBundle
// ----------------------------------------------

pub struct UiRenderFrameBundle<'ui> {
    renderer: &'ui UiRenderer,
    ctx: &'ui mut imgui::Context,
    render_sys: RcMut<RenderSystem>,
}

impl<'ui> UiRenderFrameBundle<'ui> {
    pub fn new(renderer: &'ui UiRenderer,
               ctx: &'ui mut imgui::Context,
               render_sys: RcMut<RenderSystem>) -> Self
    {
         Self { renderer, ctx, render_sys }
    }

    pub fn render(&mut self) {
        self.renderer.render(&mut self.render_sys, self.ctx);
    }
}

// ----------------------------------------------
// UiRenderer
// ----------------------------------------------

pub struct UiRenderer {
    font_atlas_tex_handle: TextureHandle,
}

impl UiRenderer {
    pub fn new(render_sys: &mut RenderSystem, ctx: &mut imgui::Context) -> Self {
        let tex_cache = render_sys.texture_cache_mut();

        let font_atlas = ctx.fonts();
        let font_atlas_texture = font_atlas.build_rgba32_texture();
        let font_atlas_size = Size::new(font_atlas_texture.width as i32, font_atlas_texture.height as i32);

        let font_atlas_tex_handle = tex_cache.new_uninitialized_texture(
            "ui_font_atlas",
            font_atlas_size,
            Some(TextureSettings {
                filter: TextureFilter::Linear,
                wrap_mode: TextureWrapMode::ClampToEdge,
                mipmaps: false,
            })
        );

        tex_cache.update_texture(
            font_atlas_tex_handle,
            0,
            0,
            font_atlas_size,
            0,
            font_atlas_texture.data
        );

        font_atlas.tex_id = UiTextureHandle::new(font_atlas_tex_handle.pack());

        Self { font_atlas_tex_handle }
    }

    pub fn render(&self, render_sys: &mut RenderSystem, ctx: &mut imgui::Context) {
        let [width,   height]  = ctx.io().display_size;
        let [scale_w, scale_h] = ctx.io().display_framebuffer_scale;

        let fb_width  = width  * scale_w;
        let fb_height = height * scale_h;

        let draw_data = ctx.render();

        if draw_data.total_idx_count <= 0 || draw_data.total_vtx_count <= 0 {
            return; // nothing to render.
        }

        render_sys.begin_ui_render();

        for draw_list in draw_data.draw_lists() {
            let vtx_buffer = draw_list.vtx_buffer();
            let idx_buffer = draw_list.idx_buffer();

            if vtx_buffer.is_empty() || idx_buffer.is_empty() {
                continue; // nothing to render for this draw list.
            }

            render_sys.set_ui_draw_buffers(vtx_buffer, idx_buffer);

            for cmd in draw_list.commands() {
                match cmd {
                    imgui::DrawCmd::Elements { count, cmd_params } => {
                        Self::execute_draw_command(
                            render_sys,
                            &UiDrawArgs {
                                scale_w,
                                scale_h,
                                fb_width,
                                fb_height,
                                count,
                                cmd_params,
                            }
                        );
                    }
                    // These are not required by our UI renderer.
                    imgui::DrawCmd::ResetRenderState => {
                        unimplemented!("Haven't implemented imgui::DrawCmd::ResetRenderState yet!");
                    }
                    imgui::DrawCmd::RawCallback { .. } => {
                        unimplemented!("Haven't implemented imgui::DrawCmd::RawCallback yet!");
                    }
                }
            }
        }

        render_sys.end_ui_render();
    }

    fn execute_draw_command(render_sys: &mut RenderSystem, args: &UiDrawArgs) {
        if args.count == 0 {
            return;
        }

        let params = &args.cmd_params;

        // Compute clip rect in framebuffer space (top-left origin, matching ImGui).
        let clip_min_x = (params.clip_rect[0] * args.scale_w).max(0.0);
        let clip_min_y = (params.clip_rect[1] * args.scale_h).max(0.0);
        let clip_max_x = (params.clip_rect[2] * args.scale_w).min(args.fb_width);
        let clip_max_y = (params.clip_rect[3] * args.scale_h).min(args.fb_height);

        let clip_rect = Rect::from_pos_and_size(
            Vec2::new(clip_min_x, clip_min_y),
            Vec2::new(clip_max_x - clip_min_x, clip_max_y - clip_min_y),
        );

        let first_index = params.idx_offset as u32;
        let index_count = args.count as u32;
        let texture = TextureHandle::unpack(params.texture_id.id());

        // Issue draw call, rendering the previously set ui vertex and index buffers.
        render_sys.draw_ui_elements(first_index, index_count, texture, clip_rect);
    }
}
