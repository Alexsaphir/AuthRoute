//! The shared parse / check / evaluate path for AuthRoute policies (ADR-0006).
//!
//! Both the admission webhook (M6) and the request-time evaluator (M2) call
//! these functions, so a policy that passes admission is guaranteed to compile
//! and evaluate identically on the hot path.
//!
//! ## CEL "type-checking" caveat
//!
//! ADR-0006 calls for static type-checking of CEL against the fixed activation
//! schema (`user: string`, `groups: list<string>`, `claims: map<string, dyn>`).
//! The `cel` crate (`cel-rust` 0.13) ships a parser and interpreter but **no
//! standalone static type checker**. We therefore approximate it: a policy is
//! accepted only if it (a) parses and (b) evaluates to a `bool` against a
//! representative [`sample_subject`] activation. This catches syntax errors,
//! references to unknown variables, and obvious type errors (e.g. comparing the
//! `groups` list to a string), but not type errors that only surface for
//! specific data. The gap is recorded for a future ADR addendum.

use std::collections::HashMap;

use cel::{Context, Program, Value};
use regex::Regex;

use crate::authpolicy::Subject;

/// Why a policy or path regex was rejected. One variant per failure mode, with
/// messages that state what failed and how to fix it (kopiur discipline).
#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    /// The CEL expression did not parse.
    #[error("policy does not parse as CEL: {0}")]
    Parse(String),

    /// The CEL expression parsed but failed to evaluate against the activation
    /// (unknown variable, type mismatch, …).
    #[error("policy fails to evaluate against the Subject activation: {0}")]
    Evaluate(String),

    /// The CEL expression evaluated to a non-boolean value.
    #[error("policy must evaluate to a bool, got {0}")]
    NotBoolean(String),

    /// The path regex did not compile.
    #[error("pathRegex is not a valid regular expression: {0}")]
    Regex(String),
}

/// A policy that has been compiled and proven to yield a `bool`. Compile once,
/// evaluate per request.
pub struct CompiledPolicy {
    program: Program,
}

impl std::fmt::Debug for CompiledPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledPolicy").finish_non_exhaustive()
    }
}

impl CompiledPolicy {
    /// Evaluate the policy against `subject`; `true` allows, `false` denies.
    pub fn evaluate(&self, subject: &Subject) -> Result<bool, PolicyError> {
        let value = self
            .program
            .execute(&activation(subject))
            .map_err(|e| PolicyError::Evaluate(e.to_string()))?;
        match value {
            Value::Bool(b) => Ok(b),
            other => Err(PolicyError::NotBoolean(format!("{other:?}"))),
        }
    }
}

/// Parse, check, and compile a CEL policy expression (ADR-0002 §D3, ADR-0006).
///
/// See the module docs for the type-checking caveat.
pub fn compile_policy(expr: &str) -> Result<CompiledPolicy, PolicyError> {
    let program = Program::compile(expr).map_err(|e| PolicyError::Parse(e.to_string()))?;
    let compiled = CompiledPolicy { program };
    // Dry-run against a representative activation to approximate type-checking
    // and to reject expressions that don't yield a bool.
    compiled.evaluate(&sample_subject())?;
    Ok(compiled)
}

/// Compile a path regex with the same engine used at request time (ADR-0006).
pub fn compile_path_regex(pattern: &str) -> Result<Regex, PolicyError> {
    Regex::new(pattern).map_err(|e| PolicyError::Regex(e.to_string()))
}

/// Build the CEL activation for a [`Subject`]: `user`, `groups`, and the
/// reserved `claims` map (ADR-0002 §D4).
fn activation(subject: &Subject) -> Context<'static> {
    let mut ctx = Context::default();
    // These inserts only fail on a name collision, which cannot happen here.
    let _ = ctx.add_variable("user", subject.username.clone());
    let _ = ctx.add_variable("groups", subject.groups.clone());
    // `claims` is reserved (ADR-0002 §D4) and not yet populated; declare it as
    // an (empty) map so policies referencing it parse and evaluate.
    let _ = ctx.add_variable("claims", HashMap::<String, String>::new());
    ctx
}

/// A representative [`Subject`] used to dry-run policies during compilation.
pub fn sample_subject() -> Subject {
    Subject {
        username: "sample@example.com".to_string(),
        groups: vec!["users".to_string(), "admins".to_string()],
        name: "Sample User".to_string(),
        email: "sample@example.com".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_common_policies() {
        // Mirrors the table in docs/research/cel-policy.md.
        for expr in [
            r#"user == "alice@example.com""#,
            r#""admins" in groups"#,
            r#""admins" in groups || user == "alice@example.com""#,
            r#"user == "bob" && "admins" in groups"#,
            r#"user != """#,
            "true",
            "false",
        ] {
            assert!(compile_policy(expr).is_ok(), "should compile: {expr}");
        }
    }

    #[test]
    fn rejects_unparseable_policy() {
        let err = compile_policy(r#"user == "#).unwrap_err();
        assert!(matches!(err, PolicyError::Parse(_)), "got {err:?}");
    }

    #[test]
    fn rejects_unknown_variable() {
        let err = compile_policy("nope == 1").unwrap_err();
        assert!(matches!(err, PolicyError::Evaluate(_)), "got {err:?}");
    }

    #[test]
    fn rejects_non_boolean_policy() {
        let err = compile_policy(r#""just a string""#).unwrap_err();
        assert!(matches!(err, PolicyError::NotBoolean(_)), "got {err:?}");
    }

    #[test]
    fn evaluates_group_membership() {
        let policy = compile_policy(r#""admins" in groups"#).unwrap();
        let mut subject = sample_subject();
        assert!(policy.evaluate(&subject).unwrap());
        subject.groups = vec!["users".to_string()];
        assert!(!policy.evaluate(&subject).unwrap());
    }

    #[test]
    fn valid_and_invalid_regexes() {
        assert!(compile_path_regex(r"^/public(/.*)?$").is_ok());
        let err = compile_path_regex(r"^/api(/.*$").unwrap_err();
        assert!(matches!(err, PolicyError::Regex(_)), "got {err:?}");
    }
}
