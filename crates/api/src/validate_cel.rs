use std::collections::HashMap;

use cel::{Context, Program, Value};

use crate::Subject;

/// Why a CEL policy expression was rejected. One variant per failure mode, with
/// messages that state what failed and how to fix it.
#[derive(Debug, thiserror::Error)]
#[allow(clippy::upper_case_acronyms)] // CEL is the policy language (ADR-0006).
pub enum CELPolicyError {
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
}

/// A policy that has been compiled and proven to yield a `bool`. Compile once,
/// evaluate per request.
pub struct CompiledCELPolicy {
    program: Program,
}

impl std::fmt::Debug for CompiledCELPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledCELPolicy").finish_non_exhaustive()
    }
}

impl CompiledCELPolicy {
    /// Evaluate the policy against `subject`; `true` allows, `false` denies.
    pub fn evaluate(&self, subject: &Subject) -> Result<bool, CELPolicyError> {
        let value = self
            .program
            .execute(&activation(subject))
            .map_err(|e| CELPolicyError::Evaluate(e.to_string()))?;
        match value {
            Value::Bool(b) => Ok(b),
            other => Err(CELPolicyError::NotBoolean(format!("{other:?}"))),
        }
    }
}

/// Parse, check, and compile a CEL policy expression (ADR-0002 §D3, ADR-0006).
///
/// See the module docs for the type-checking caveat.
pub fn compile_cel_policy(expr: &str) -> Result<CompiledCELPolicy, CELPolicyError> {
    let program = Program::compile(expr).map_err(|e| CELPolicyError::Parse(e.to_string()))?;
    let compiled = CompiledCELPolicy { program };
    // Dry-run against a representative activation to approximate type-checking
    // and to reject expressions that don't yield a bool.
    compiled.evaluate(&sample_subject())?;
    Ok(compiled)
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
            assert!(compile_cel_policy(expr).is_ok(), "should compile: {expr}");
        }
    }

    #[test]
    fn rejects_unparseable_policy() {
        let err = compile_cel_policy(r#"user == "#).unwrap_err();
        assert!(matches!(err, CELPolicyError::Parse(_)), "got {err:?}");
    }

    #[test]
    fn rejects_unknown_variable() {
        let err = compile_cel_policy("nope == 1").unwrap_err();
        assert!(matches!(err, CELPolicyError::Evaluate(_)), "got {err:?}");
    }

    #[test]
    fn rejects_non_boolean_policy() {
        let err = compile_cel_policy(r#""just a string""#).unwrap_err();
        assert!(matches!(err, CELPolicyError::NotBoolean(_)), "got {err:?}");
    }

    #[test]
    fn evaluates_group_membership() {
        let policy = compile_cel_policy(r#""admins" in groups"#).unwrap();
        let mut subject = sample_subject();
        assert!(policy.evaluate(&subject).unwrap());
        subject.groups = vec!["users".to_string()];
        assert!(!policy.evaluate(&subject).unwrap());
    }
}
