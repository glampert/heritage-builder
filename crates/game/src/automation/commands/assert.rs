// Assertions for scripts: run a query command and compare a value in its result, or compare a
// screenshot with a golden image. A failed assertion fails the command with AssertionFailed,
// which fails the script.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Value, json};

use engine::{
    cli,
    screenshot::{self, ImageComparison, ScreenshotOutput, ScreenshotRequest},
};

use super::observe::screenshot_scale;
use crate::automation::{
    CommandContext,
    CommandDef,
    CommandError,
    CommandOutput,
    CommandResult,
    ErrorCode,
    ParamDef,
    done,
    parse_params,
    pending,
    query::{Check, CompareOp},
};

macro_rules! assert_command {
    ($name:literal, $help:literal, $run:ident) => {
        CommandDef { name: $name, help: $help, params: ASSERT_PARAMS, run: $run }
    };
}

const ASSERT_PARAMS: &[ParamDef] = &[
    ParamDef { name: "query", kind: "string", required: true, help: "Command whose result to check, e.g. session.state." },
    ParamDef { name: "args", kind: "object", required: false, help: "The query's parameters." },
    ParamDef { name: "path", kind: "string", required: false, help: "JSON pointer into the result, e.g. /population." },
    ParamDef { name: "value", kind: "any", required: false, help: "Expected value. Required except for assert.exists." },
];

pub(super) const COMMANDS: &[CommandDef] = &[
    assert_command!("assert.eq", "Fail unless the value equals value (numbers compare by value).", cmd_assert_eq),
    assert_command!("assert.ne", "Fail if the value equals value.", cmd_assert_ne),
    assert_command!("assert.gt", "Fail unless the value is greater than value.", cmd_assert_gt),
    assert_command!("assert.ge", "Fail unless the value is greater than or equal to value.", cmd_assert_ge),
    assert_command!("assert.lt", "Fail unless the value is less than value.", cmd_assert_lt),
    assert_command!("assert.le", "Fail unless the value is less than or equal to value.", cmd_assert_le),
    assert_command!("assert.exists", "Fail unless the query succeeds and has a non-null value at path.", cmd_assert_exists),
    CommandDef {
        name: "assert.image",
        help: "Take a screenshot and fail unless it matches a golden PNG. Only for goldens made on the same machine \
               and render backend: GPUs and drivers render slightly differently.",
        params: &[
            ParamDef {
                name: "golden",
                kind: "string",
                required: true,
                help: "PNG to compare with, relative to the directory the game was started from.",
            },
            ParamDef {
                name: "tolerance",
                kind: "int",
                required: false,
                help: "Largest difference in a color channel (0-255) that still matches. Default 0.",
            },
            ParamDef {
                name: "max_diff_pixels",
                kind: "int",
                required: false,
                help: "How many pixels may differ by more than tolerance. Default 0.",
            },
            ParamDef {
                name: "path",
                kind: "string",
                required: false,
                help: "Also save the screenshot here, and if pixels differ, NAME-diff.png next to it with them in red.",
            },
            ParamDef { name: "include_ui", kind: "bool", required: false, help: "False leaves the UI out. Default true." },
            ParamDef { name: "scale", kind: "number|string", required: false, help: "As for screenshot. Default 1." },
            ParamDef {
                name: "update",
                kind: "bool",
                required: false,
                help: "True saves the screenshot as the golden instead of comparing, e.g. to create it.",
            },
        ],
        run: cmd_assert_image,
    },
];

fn run_assert(context: &mut CommandContext, params: Value, op: CompareOp) -> CommandResult {
    let check: Check = parse_params(params)?;
    let actual = check.assert(context, op)?;
    done(json!({ "actual": actual }))
}

fn cmd_assert_eq(context: &mut CommandContext, params: Value) -> CommandResult {
    run_assert(context, params, CompareOp::Eq)
}

fn cmd_assert_ne(context: &mut CommandContext, params: Value) -> CommandResult {
    run_assert(context, params, CompareOp::Ne)
}

fn cmd_assert_gt(context: &mut CommandContext, params: Value) -> CommandResult {
    run_assert(context, params, CompareOp::Gt)
}

fn cmd_assert_ge(context: &mut CommandContext, params: Value) -> CommandResult {
    run_assert(context, params, CompareOp::Ge)
}

fn cmd_assert_lt(context: &mut CommandContext, params: Value) -> CommandResult {
    run_assert(context, params, CompareOp::Lt)
}

fn cmd_assert_le(context: &mut CommandContext, params: Value) -> CommandResult {
    run_assert(context, params, CompareOp::Le)
}

fn cmd_assert_exists(context: &mut CommandContext, params: Value) -> CommandResult {
    run_assert(context, params, CompareOp::Exists)
}

fn cmd_assert_image(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        golden: PathBuf,
        #[serde(default)]
        tolerance: u8,
        #[serde(default)]
        max_diff_pixels: u64,
        path: Option<PathBuf>,
        #[serde(default = "default_include_ui")]
        include_ui: bool,
        scale: Option<Value>,
        #[serde(default)]
        update: bool,
    }

    fn default_include_ui() -> bool {
        true
    }

    let params: Params = parse_params(params)?;
    let scale = screenshot_scale(params.scale.as_ref())?;

    let check = ImageCheck {
        golden_path: cli::resolve_launch_path(&params.golden),
        save_path: params.path.as_deref().map(cli::resolve_launch_path),
        tolerance: params.tolerance,
        max_diff_pixels: params.max_diff_pixels,
    };

    // Read the golden first, so a missing one fails before the capture.
    let golden = if params.update {
        None
    } else {
        Some(std::fs::read(&check.golden_path).map_err(|err| {
            CommandError::failed(format!(
                "Can't read the golden image {}: {err}. update=true saves the screenshot as the golden.",
                check.golden_path.display()
            ))
        })?)
    };

    let output = ScreenshotOutput::Memory;
    let id = context.engine.request_screenshot(ScreenshotRequest { include_ui: params.include_ui, scale, output });

    pending(move |context| {
        let result = context.engine.take_screenshot_result(id)?;
        let png = match result {
            Ok(screenshot) => screenshot.png.unwrap_or_default(),
            Err(err) => return Some(Err(CommandError::failed(err))),
        };
        Some(check.run(&png, golden.as_deref()))
    })
}

struct ImageCheck {
    golden_path: PathBuf,
    save_path: Option<PathBuf>,
    tolerance: u8,
    max_diff_pixels: u64,
}

impl ImageCheck {
    // Compares the screenshot with the golden, or makes it the golden when there's none to compare.
    fn run(&self, png: &[u8], golden: Option<&[u8]>) -> CommandOutput {
        if let Some(save_path) = &self.save_path {
            write_png(save_path, png)?;
        }

        let golden_name = self.golden_path.display().to_string();

        let Some(golden) = golden else {
            write_png(&self.golden_path, png)?;
            return Ok(json!({ "updated": golden_name }));
        };

        let (diff_pixels, max_channel_diff, diff_png) =
            match screenshot::compare_png(png, golden, self.tolerance).map_err(CommandError::failed)? {
                ImageComparison::Compared { diff_pixels, max_channel_diff, diff_png } => (diff_pixels, max_channel_diff, diff_png),
                ImageComparison::SizeMismatch { actual, expected } => {
                    return Err(CommandError::new(
                        ErrorCode::AssertionFailed,
                        format!("The screenshot is {actual} and the golden image {golden_name} is {expected}."),
                    ));
                }
            };

        let diff_path = match (&self.save_path, diff_png) {
            (Some(save_path), Some(diff_png)) => {
                let diff_path = diff_image_path(save_path);
                write_png(&diff_path, &diff_png)?;
                Some(diff_path.display().to_string())
            }
            _ => None,
        };

        if diff_pixels > self.max_diff_pixels {
            let diff_note = diff_path.map(|path| format!(" Differences in red: {path}.")).unwrap_or_default();
            return Err(CommandError::new(
                ErrorCode::AssertionFailed,
                format!(
                    "{diff_pixels} pixels differ from {golden_name} by more than {} (at most {} may). The largest \
                     channel difference is {max_channel_diff}.{diff_note}",
                    self.tolerance, self.max_diff_pixels
                ),
            ));
        }

        Ok(json!({
            "golden": golden_name,
            "diff_pixels": diff_pixels,
            "max_channel_diff": max_channel_diff,
            "diff_image": diff_path,
        }))
    }
}

// NAME.png -> NAME-diff.png
fn diff_image_path(path: &Path) -> PathBuf {
    let stem = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!("{stem}-diff.png"))
}

fn write_png(path: &Path, png: &[u8]) -> Result<(), CommandError> {
    let write = || -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, png)
    };
    write().map_err(|err| CommandError::failed(format!("Can't write {}: {err}", path.display())))
}
