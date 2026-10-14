---
paths:
  - "crates/engine/src/render/**"
---

# Renderer notes

Last verified against the code on 2026-10-14.

## Backends

- Frontend: `crates/engine/src/render/mod.rs` (`RenderSystem`), dispatching via `#[enum_dispatch]` to `RenderSystemBackendImpl`.
- `render/wgpu/` builds on every target; `render/opengl/` is `#[cfg(feature = "desktop")]` only.
- `RenderApi` defaults to `Wgpu`. On desktop it comes from `engine.render_api` in `assets/configs/game/configs.json`, which ships as `OpenGl`. The GLFW app backend (`engine.app_api = "Glfw"`) forces OpenGL with a warning (`engine/src/runner/desktop.rs`). Web always uses Wgpu, built from GPU resources that `WebRunner` creates asynchronously.

## Frame structure

- Draw calls are batched per primitive in `DrawBatch`es: sprites (triangles), lines, points, plus a separate ImGui UI batch. Both backends have sprite, line and point pipelines.
- The scene renders into an offscreen `RenderTarget` and is then blitted to the screen (OpenGL `RenderTarget::blit_filter`; Wgpu `blit_pipeline` + `blit.wgsl`).
- Wgpu records UI draws as commands and replays them in a dedicated UI pass in `end_frame`.

## Frame capture (screenshots)

- `RenderSystem::request_capture` (`render/capture.rs`) captures the frame being drawn, as presented that frame: the final image, or the blitted scene alone for scene-only requests. Results are RGBA8, top row first, opaque, at framebuffer size. Requests are split by kind (with UI, scene only), and each kind is captured at most once per frame. OpenGL completes them inside `end_frame`; Wgpu encodes the copies before present and completes them once the readback buffer is mapped.
- OpenGL reads the back buffer with `glReadPixels`: right after the scene blit for scene-only captures, and after the UI for the rest. Rows are flipped, since GL reads bottom-up.
- Wgpu can't copy from the surface: it's configured for `RENDER_ATTACHMENT` only, the only usage some backends (web) allow. On capture frames, the blit pass and the UI pass are encoded again into a framebuffer-sized capture texture in the surface format, then copied into a `MAP_READ` buffer with rows padded to 256 bytes. Desktop waits on the device so the capture finishes that frame. On web, the buffer is mapped later from the browser's event loop. BGRA surfaces are swizzled to RGBA. Formats other than 8-bit RGBA/BGRA return an error.
- The `Engine` drains finished captures every frame (`take_finished_captures`). Use `Engine::request_screenshot`, not the render system directly.

## Texture cache

- `TextureCache` (`render/texture.rs`) is a `Slab` of backend textures plus a pre-hashed name→index lookup. It has no LRU and no eviction. The `tex_cache_initial_capacity: 128` is only a capacity hint.
- Two textures whose names hash to the same value cause a panic ("TextureCache key collision").
- Built-in 8×8 handles: `TextureHandle::Invalid` (dummy) and `TextureHandle::White`.

## Culling

- Sprites, lines and points that are fully outside the viewport are dropped. The helpers are `is_rect_fully_offscreen`, `is_line_fully_offscreen` and `is_point_fully_offscreen` in `render/mod.rs`, called from the frontend and from both backends.
- On the game side, the tile map is drawn and animated only over the camera's visible `CellRange` (`GameSession::draw_tile_map` → `TileMapRenderer::draw_map` in `game/src/tile/rendering.rs`; `GameSession::update_anims`). There's no spatial acceleration structure beyond that.
