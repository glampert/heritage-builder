---
paths:
  - "crates/engine/src/render/**"
---

# Renderer notes

Last verified against the code on 2026-10-08.

## Backends

- Frontend: `crates/engine/src/render/mod.rs` (`RenderSystem`), dispatching via `#[enum_dispatch]` to `RenderSystemBackendImpl`.
- `render/wgpu/` builds on every target; `render/opengl/` is `#[cfg(feature = "desktop")]` only.
- `RenderApi` defaults to `Wgpu`. On desktop it comes from `engine.render_api` in `assets/configs/game/configs.json`, which ships as `OpenGl`. The GLFW app backend (`engine.app_api = "Glfw"`) forces OpenGL with a warning (`engine/src/runner/desktop.rs`). Web always uses Wgpu, built from GPU resources that `WebRunner` creates asynchronously.

## Frame structure

- Draw calls are batched per primitive in `DrawBatch`es: sprites (triangles), lines, points, plus a separate ImGui UI batch. Both backends have sprite, line and point pipelines.
- The scene renders into an offscreen `RenderTarget` and is then blitted to the screen (OpenGL `RenderTarget::blit_filter`; Wgpu `blit_pipeline` + `blit.wgsl`).
- Wgpu records UI draws as commands and replays them in a dedicated UI pass in `end_frame`.

## Texture cache

- `TextureCache` (`render/texture.rs`) is a `Slab` of backend textures plus a pre-hashed name→index lookup. It has no LRU and no eviction. The `tex_cache_initial_capacity: 128` is only a capacity hint.
- Two textures whose names hash to the same value cause a panic ("TextureCache key collision").
- Built-in 8×8 handles: `TextureHandle::Invalid` (dummy) and `TextureHandle::White`.

## Culling

- Sprites, lines and points that are fully outside the viewport are dropped. The helpers are `is_rect_fully_offscreen`, `is_line_fully_offscreen` and `is_point_fully_offscreen` in `render/mod.rs`, called from the frontend and from both backends.
- On the game side, the tile map is drawn and animated only over the camera's visible `CellRange` (`GameSession::draw_tile_map` → `TileMapRenderer::draw_map` in `game/src/tile/rendering.rs`; `GameSession::update_anims`). There's no spatial acceleration structure beyond that.
