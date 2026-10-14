// Checks on command results, shared by assert.* and wait.until: run a command that finishes at
// once (the query), pick a value out of its result with a JSON pointer, and compare it.

use serde::Deserialize;
use serde_json::{Map, Value};
use strum::{Display, EnumString};

use super::{CommandContext, CommandError, ErrorCode, Outcome, run_command};

#[derive(Copy, Clone, PartialEq, Eq, Debug, Display, EnumString, Deserialize)]
#[strum(serialize_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum CompareOp {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    Exists, // The value is there and isn't null.
}

impl CompareOp {
    fn symbol(self) -> &'static str {
        match self {
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Exists => "exists",
        }
    }
}

// Parameters naming the value to check. Part of each assert.* and wait.until command's params.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub query: String,
    #[serde(default)]
    pub args: Option<Map<String, Value>>,
    #[serde(default)]
    pub path: String, // JSON pointer into the query's result. Empty for the whole result.
    #[serde(default)]
    pub value: Option<Value>, // Expected value. Not used by exists.
}

pub enum CheckResult {
    Passed(Value),          // The value that passed, null if missing.
    NotYet(String),         // Why it didn't pass.
}

impl Check {
    pub fn validate(&self, op: CompareOp) -> Result<(), CommandError> {
        if op != CompareOp::Exists && self.value.is_none() {
            return Err(CommandError::invalid_params(format!("'{op}' needs a value to compare with.")));
        }
        if self.query.starts_with("assert.") || self.query.starts_with("wait.") {
            return Err(CommandError::invalid_params(format!("'{}' can't be used as a query.", self.query)));
        }
        Ok(())
    }

    pub fn run(&self, context: &mut CommandContext, op: CompareOp) -> Result<CheckResult, CommandError> {
        let args = self.args.clone().map(Value::Object).unwrap_or(Value::Null);

        let result = match run_command(context, &self.query, args)? {
            Outcome::Done(result) => result,
            Outcome::Pending(_) => {
                return Err(CommandError::invalid_params(format!(
                    "'{}' doesn't finish at once, so it can't be used as a query.",
                    self.query
                )));
            }
        };

        let path = normalize_pointer(&self.path);
        let actual = result.pointer(&path);

        let passed = match op {
            CompareOp::Exists => actual.is_some_and(|actual| !actual.is_null()),
            _ => {
                let expected = self.value.as_ref().unwrap_or(&Value::Null);
                match actual {
                    Some(actual) => compare(op, actual, expected).map_err(CommandError::invalid_params)?,
                    None => {
                        return Ok(CheckResult::NotYet(format!("'{}' returned no value at '{path}'.", self.query)));
                    }
                }
            }
        };

        let actual = actual.cloned().unwrap_or(Value::Null);

        if passed {
            Ok(CheckResult::Passed(actual))
        } else if op == CompareOp::Exists {
            Ok(CheckResult::NotYet(format!("'{}' returned no value at '{path}'.", self.query)))
        } else {
            let expected = self.value.as_ref().unwrap_or(&Value::Null);
            let target = if path.is_empty() { self.query.clone() } else { format!("{} {path}", self.query) };
            Ok(CheckResult::NotYet(format!("Expected {target} {} {expected}, got {actual}.", op.symbol())))
        }
    }

    // For assert.*: a check that didn't pass is a failure.
    pub fn assert(&self, context: &mut CommandContext, op: CompareOp) -> Result<Value, CommandError> {
        self.validate(op)?;
        match self.run(context, op)? {
            CheckResult::Passed(actual) => Ok(actual),
            CheckResult::NotYet(message) => Err(CommandError::new(ErrorCode::AssertionFailed, message)),
        }
    }
}

// JSON pointers start with '/'. Accept "population" for "/population".
fn normalize_pointer(path: &str) -> String {
    if path.is_empty() || path.starts_with('/') { path.to_string() } else { format!("/{path}") }
}

// Numbers compare by value (2 == 2.0). Ordering works on numbers and on strings.
pub fn compare(op: CompareOp, actual: &Value, expected: &Value) -> Result<bool, String> {
    use std::cmp::Ordering;

    let ordering = || -> Result<Ordering, String> {
        match (actual, expected) {
            (Value::Number(a), Value::Number(b)) => {
                let (a, b) = (a.as_f64().unwrap_or(f64::NAN), b.as_f64().unwrap_or(f64::NAN));
                a.partial_cmp(&b).ok_or_else(|| format!("Can't compare {a} with {b}."))
            }
            (Value::String(a), Value::String(b)) => Ok(a.cmp(b)),
            _ => Err(format!("Can't order {actual} against {expected}: compare two numbers or two strings.")),
        }
    };

    Ok(match op {
        CompareOp::Eq => json_eq(actual, expected),
        CompareOp::Ne => !json_eq(actual, expected),
        CompareOp::Gt => ordering()? == Ordering::Greater,
        CompareOp::Ge => ordering()? != Ordering::Less,
        CompareOp::Lt => ordering()? == Ordering::Less,
        CompareOp::Le => ordering()? != Ordering::Greater,
        CompareOp::Exists => !actual.is_null(),
    })
}

// Equality where numbers compare by value, so 2 equals 2.0.
pub fn json_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => a.len() == b.len() && a.iter().zip(b).all(|(a, b)| json_eq(a, b)),
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len() && a.iter().all(|(key, a)| b.get(key).is_some_and(|b| json_eq(a, b)))
        }
        _ => a == b,
    }
}

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_compare() {
        assert_eq!(compare(CompareOp::Eq, &json!(2), &json!(2.0)), Ok(true));
        assert_eq!(compare(CompareOp::Eq, &json!([1, {"a": 2}]), &json!([1.0, {"a": 2}])), Ok(true));
        assert_eq!(compare(CompareOp::Ne, &json!("a"), &json!("b")), Ok(true));
        assert_eq!(compare(CompareOp::Gt, &json!(3), &json!(2.5)), Ok(true));
        assert_eq!(compare(CompareOp::Ge, &json!(2), &json!(2)), Ok(true));
        assert_eq!(compare(CompareOp::Lt, &json!(2), &json!(2)), Ok(false));
        assert_eq!(compare(CompareOp::Le, &json!("abc"), &json!("abd")), Ok(true));
        assert!(compare(CompareOp::Gt, &json!(true), &json!(1)).is_err());
        assert_eq!(compare(CompareOp::Exists, &json!(0), &Value::Null), Ok(true));
        assert_eq!(compare(CompareOp::Exists, &Value::Null, &Value::Null), Ok(false));
    }

    #[test]
    fn test_parse_op() {
        assert_eq!("gt".parse::<CompareOp>(), Ok(CompareOp::Gt));
        assert_eq!(serde_json::from_value::<CompareOp>(json!("exists")).unwrap(), CompareOp::Exists);
        assert!("bigger".parse::<CompareOp>().is_err());
    }

    #[test]
    fn test_normalize_pointer() {
        assert_eq!(normalize_pointer(""), "");
        assert_eq!(normalize_pointer("population"), "/population");
        assert_eq!(normalize_pointer("/map_size/0"), "/map_size/0");
    }
}
