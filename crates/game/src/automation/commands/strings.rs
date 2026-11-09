// Localized string commands (docs/plans/realm-ui.md, Text): the language, a key's text, and the
// keys looked up and not found. (Not text.rs: automation/text.rs is the console line parser.)

use std::{collections::BTreeMap, str::FromStr};

use serde::Deserialize;
use serde_json::{Value, json};
use strum::IntoEnumIterator;

use engine::ui::text::{self, UiTextCategory};

use crate::automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "text.language",
        help: "The language of the player's text and the languages loaded (assets/configs/ui/text/<language>.json); \
               with language, switch to it first. For this run only: the configs keep their language.",
        params: &[ParamDef {
            name: "language",
            kind: "string",
            required: false,
            help: "A language loaded, by file name, e.g. en.",
        }],
        run: cmd_language,
    },
    CommandDef {
        name: "text.missing",
        help: "The keys looked up since the start and missing from en.json too, which show as the key, by category \
               (missing, and their count), and the keys a language lacks and English has, which show in English \
               (untranslated, by language and category). Usable as an assert.* query, e.g. /count is 0.",
        params: &[ParamDef {
            name: "reset",
            kind: "bool",
            required: false,
            help: "Forget the keys listed so far, once they're reported.",
        }],
        run: cmd_missing,
    },
    CommandDef {
        name: "text.get",
        help: "A key's text in the current language, as the UI would show it, without counting a miss: text, and \
               language, the language it comes from (English for a fallback, null for a missing key, whose text \
               is the key).",
        params: &[
            ParamDef { name: "category", kind: "string", required: true, help: "The key's category, e.g. Menu." },
            ParamDef { name: "key", kind: "string", required: true, help: "The key, e.g. button.cancel." },
            ParamDef {
                name: "count",
                kind: "int",
                required: false,
                help: "Picks the plural form for this count, and fills {count}.",
            },
            ParamDef {
                name: "args",
                kind: "object",
                required: false,
                help: "Values for the text's {name} placeholders, e.g. {\"building\": \"Temple\"}.",
            },
        ],
        run: cmd_get,
    },
];

fn language_json() -> Value {
    json!({ "language": text::current_language(), "languages": text::languages() })
}

fn cmd_language(_context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        language: Option<String>,
    }

    let params: Params = parse_params(params)?;

    if let Some(language) = &params.language {
        text::set_language(language).map_err(CommandError::not_found)?;
    }

    done(language_json())
}

fn cmd_missing(_context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        #[serde(default)]
        reset: bool,
    }

    let params: Params = parse_params(params)?;

    let missing = text::missing_keys();
    let untranslated = text::untranslated_keys();
    if params.reset {
        text::reset_misses();
    }

    let count: usize = missing.values().map(Vec::len).sum();
    let untranslated_count: usize = untranslated.values().flat_map(BTreeMap::values).map(Vec::len).sum();
    let by_category = |keys: BTreeMap<UiTextCategory, Vec<String>>| -> Value {
        keys.into_iter().map(|(category, keys)| (category.to_string(), json!(keys))).collect::<serde_json::Map<_, _>>().into()
    };

    done(json!({
        "language": text::current_language(),
        "count": count,
        "missing": by_category(missing),
        "untranslated_count": untranslated_count,
        "untranslated": untranslated
            .into_iter()
            .map(|(language, keys)| (language, by_category(keys)))
            .collect::<serde_json::Map<_, _>>(),
    }))
}

fn cmd_get(_context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        category: String,
        key: String,
        count: Option<i64>,
        #[serde(default)]
        args: BTreeMap<String, Value>,
    }

    let params: Params = parse_params(params)?;

    let category = UiTextCategory::from_str(&params.category).map_err(|_| {
        let names: Vec<String> = UiTextCategory::iter().map(|category| category.to_string()).collect();
        CommandError::invalid_params(format!("No category '{}'. Categories: {}.", params.category, names.join(", ")))
    })?;
    if params.key.is_empty() {
        return Err(CommandError::invalid_params("Give a key, e.g. button.cancel."));
    }

    // Placeholder values as text: strings as they are, other JSON values as JSON.
    let mut args: Vec<(String, String)> = params
        .args
        .into_iter()
        .map(|(name, value)| (name, value.as_str().map_or_else(|| value.to_string(), str::to_string)))
        .collect();
    if let Some(count) = params.count
        && !args.iter().any(|(name, _)| name == "count")
    {
        args.push(("count".to_string(), count.to_string()));
    }

    let (template, language) = text::peek(category, &params.key, params.count);
    let args: Vec<(&str, &str)> = args.iter().map(|(name, value)| (name.as_str(), value.as_str())).collect();

    done(json!({
        "category": category.to_string(),
        "key": params.key,
        "text": text::fill_placeholders(&template, &args),
        "language": language,
    }))
}
