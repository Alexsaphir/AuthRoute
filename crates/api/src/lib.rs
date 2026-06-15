//! AuthRoute `api` crate — runtime-free CRD types and policy validators.
//!
//! This crate holds the `AuthPolicy` custom resource (ADR-0002), the identity
//! [`Subject`] (ADR-0003), and the single parse / type-check / evaluate path for
//! CEL policies and path regexes (ADR-0006). It deliberately has no controller
//! or async-runtime dependencies, so the controller, the admission webhook, and
//! the request-time evaluator can all reuse exactly the same validation code —
//! guaranteeing admission and request time never disagree.

mod authpolicy;
mod subject;
mod validate_cel;
mod validate_regex;

pub use authpolicy::{
    AuthPolicy, AuthPolicySpec, AuthPolicyStatus, ExtraPolicy, TargetRef, TargetRefKind,
};
pub use subject::Subject;
pub use validate_cel::{CELPolicyError, CompiledCELPolicy, compile_cel_policy, sample_subject};
pub use validate_regex::{PathRegexError, compile_path_regex};
