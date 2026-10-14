// Assertions for scripts: run a query command and compare a value in its result. A failed
// assertion fails the command with AssertionFailed, which fails the script.

use serde_json::{Value, json};

use crate::automation::{
    CommandContext,
    CommandDef,
    CommandResult,
    ParamDef,
    done,
    parse_params,
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
