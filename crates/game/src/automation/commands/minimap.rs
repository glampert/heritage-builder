// Minimap commands (docs/plans/minimap.md): read and set its state, reload its style, export its
// baked base.

use std::str::FromStr;

use serde::Deserialize;
use serde_json::{Value, json};
use strum::IntoEnumIterator;

use common::{Color, Vec2};
use engine::{cli, render::capture::CapturedImage, screenshot};

use super::{CellParam, cell_json};
use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    tile::minimap::{
        MINIMAP_ICON_DEFAULT_LIFETIME,
        MinimapIcon,
        MinimapLens,
        model::LandmarkKind,
        style::{self, MinimapRealm},
    },
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "minimap.state",
        help: "The minimap: open, lens and the lenses on offer, zoom, visible cells, the cell at the centre of its view, \
               alerts, features and landmarks, the baked texture's size and pixels per cell, the last bake's time, the widget's and the map area's sizes in points, and the style's realm.",
        params: &[],
        run: cmd_state,
    },
    CommandDef {
        name: "minimap.set",
        help: "Open or close the minimap, pick a lens, show or hide the building icons, set its zoom (stops the auto zoom) or centre its view on a cell.",
        params: &[
            ParamDef { name: "open", kind: "bool", required: false, help: "Show the minimap, or only its open button." },
            ParamDef { name: "lens", kind: "string", required: false, help: "city, sectors or water." },
            ParamDef { name: "show_landmarks", kind: "bool", required: false, help: "Show the building icons in the City view, as the toggle does." },
            ParamDef { name: "zoom", kind: "number", required: false, help: "1 shows the whole playable area; more zooms in." },
            ParamDef { name: "auto_zoom", kind: "bool", required: false, help: "Zoom to about 95 visible cells, as on opening." },
            ParamDef { name: "center", kind: "cell", required: false, help: "[x, y]: centre the view there (when zoomed in)." },
        ],
        run: cmd_set,
    },
    CommandDef {
        name: "minimap.alert",
        help: "Raise an alert at a cell, as the game's events do: a ping on the minimap, and an entry the alerts tool \
               goes to (kept 60 seconds).",
        params: &[ParamDef { name: "cell", kind: "cell", required: true, help: "[x, y]." }],
        run: cmd_alert,
    },
    CommandDef {
        name: "minimap.reload_style",
        help: "Load the minimap's style file again (assets/configs/ui/minimap/<realm>.json), e.g. after editing it, or \
               another realm's. Keeps the current style if the file fails to load.",
        params: &[ParamDef { name: "realm", kind: "string", required: false, help: "dailong, kambara or tampadesa. Default: the current one." }],
        run: cmd_reload_style,
    },
    CommandDef {
        name: "minimap.export",
        help: "Save the minimap's baked base texture as a PNG, to compare with the mockups.",
        params: &[
            ParamDef { name: "path", kind: "string", required: false, help: "PNG file, relative to the directory the game was started from. Default: out/minimap.png." },
            ParamDef { name: "inline", kind: "bool", required: false, help: "True returns the PNG as base64 (png_base64) instead of writing a file." },
        ],
        run: cmd_export,
    },
];

fn cmd_state(context: &mut CommandContext, _params: Value) -> CommandResult {
    context.require_map()?;
    done(minimap_state(context))
}

fn minimap_state(context: &CommandContext) -> Value {
    let minimap = context.session.tile_map().minimap();
    let view = minimap.view();
    let center = crate::tile::minimap::widget::world_to_cell(view.center);
    let size = minimap.size_in_cells();
    let texture = minimap.texture_size();

    let landmarks: Vec<Value> = LandmarkKind::iter()
        .flat_map(|kind| {
            minimap.features().iter().filter(move |feature| feature.landmark == Some(kind)).map(move |feature| {
                json!({ "kind": kind.to_string(), "cell": cell_json(feature.base_cell) })
            })
        })
        .collect();

    json!({
        "open": minimap.is_open(),
        "lens": minimap.lens().to_string(),
        "show_landmarks": minimap.shows_landmarks(),
        "lenses": MinimapLens::available().iter().map(|lens| lens.to_string()).collect::<Vec<_>>(),
        "zoom": view.zoom,
        "auto_zoom": view.auto_zoom,
        "visible_cells": view.visible_cells(),
        "center_cell": [center.x.round() as i32, center.y.round() as i32],
        "alerts": minimap.alert_count(),
        "features": minimap.features().len(),
        "landmarks": landmarks,
        "map_size": [size.width, size.height],
        "texture_size": [texture.width, texture.height],
        "px_per_cell": minimap.px_per_cell(),
        "window_size": [minimap.layout().window.width(), minimap.layout().window.height()],
        "map_rect_size": [minimap.layout().map.width(), minimap.layout().map.height()],
        "last_bake_ms": minimap.last_bake_secs() * 1000.0,
        "realm": style::current_realm().to_string(),
    })
}

fn cmd_set(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        open: Option<bool>,
        lens: Option<String>,
        show_landmarks: Option<bool>,
        zoom: Option<f32>,
        auto_zoom: Option<bool>,
        center: Option<CellParam>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    let lens = match &params.lens {
        Some(name) => {
            let lens = MinimapLens::from_str(name).ok().filter(|lens| MinimapLens::available().contains(lens));
            Some(lens.ok_or_else(|| {
                let names: Vec<String> = MinimapLens::available().iter().map(|lens| lens.to_string()).collect();
                CommandError::invalid_params(format!("Unknown lens '{name}'. Lenses: {}.", names.join(", ")))
            })?)
        }
        None => None,
    };
    if let Some(zoom) = params.zoom
        && !(zoom.is_finite() && zoom >= 1.0)
    {
        return Err(CommandError::invalid_params(format!("Zoom must be 1 or more, got {zoom}.")));
    }
    if let Some(center) = params.center {
        context.require_cell_in_map(center.0)?;
    }

    let minimap = context.session.tile_map_mut().minimap_mut();
    if let Some(open) = params.open {
        minimap.set_open(open);
    }
    if let Some(lens) = lens {
        minimap.set_lens(lens);
    }
    if let Some(show) = params.show_landmarks {
        minimap.set_show_landmarks(show);
    }
    if let Some(auto_zoom) = params.auto_zoom {
        minimap.view_mut().auto_zoom = auto_zoom;
    }
    if let Some(zoom) = params.zoom {
        let view = minimap.view_mut();
        view.auto_zoom = false;
        view.set_zoom(zoom);
    }
    if let Some(center) = params.center {
        let view = minimap.view_mut();
        view.auto_scroll = false; // Or it scrolls back to the camera.
        view.center_on_cell(Vec2::new(center.0.x as f32, center.0.y as f32));
    }

    done(minimap_state(context))
}

fn cmd_alert(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        cell: CellParam,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;
    context.require_cell_in_map(params.cell.0)?;

    let minimap = context.session.tile_map_mut().minimap_mut();
    minimap.push_icon(MinimapIcon::Alert, params.cell.0, Color::white(), MINIMAP_ICON_DEFAULT_LIFETIME);
    done(json!({ "alerts": minimap.alert_count() }))
}

fn cmd_reload_style(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        realm: Option<String>,
    }

    let params: Params = parse_params(params)?;
    let realm = match &params.realm {
        Some(name) => Some(MinimapRealm::from_str(name).map_err(|_| {
            CommandError::invalid_params(format!("Unknown realm '{name}'. Realms: {}.", MinimapRealm::names().join(", ")))
        })?),
        None => None,
    };

    let realm = style::reload(realm).map_err(CommandError::failed)?;
    let _ = context;
    done(json!({ "realm": realm.to_string() }))
}

fn cmd_export(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        path: Option<String>,
        #[serde(default)]
        inline: bool,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    let (pixels, size) = context.session.tile_map_mut().minimap_mut().baked_pixels();
    let image = CapturedImage { size, scale: Vec2::one(), pixels: pixels.to_vec() };
    let png = screenshot::encode_png(&image).map_err(CommandError::failed)?;

    let mut reply = json!({ "size": [size.width, size.height] });
    if params.inline {
        reply["png_base64"] = json!(screenshot::base64_encode(&png));
    } else {
        let path = cli::resolve_launch_path(std::path::Path::new(params.path.as_deref().unwrap_or("out/minimap.png")));
        let write = || -> std::io::Result<()> {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, &png)
        };
        write().map_err(|err| CommandError::failed(format!("Can't write {}: {err}", path.display())))?;
        reply["path"] = json!(path.display().to_string());
    }
    done(reply)
}
