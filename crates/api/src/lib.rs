//! AuthRoute `api` crate — runtime-free CRD types and policy validators.
//!
//! This crate holds the `AuthPolicy` custom resource (ADR-0002), the identity
//! [`Subject`] (ADR-0003), and the single parse / type-check / evaluate path for
//! CEL policies and path regexes (ADR-0006). It deliberately has no controller
//! or async-runtime dependencies so the controller, the admission webhook, and
//! the request-time evaluator can all reuse exactly the same validation code —
//! guaranteeing admission and request time never disagree.

mod authpolicy;
mod validate;

pub use authpolicy::{
    AuthPolicy, AuthPolicySpec, AuthPolicyStatus, ExtraPolicy, Subject, TargetRef, TargetRefKind,
};
pub use validate::{
    CompiledPolicy, PolicyError, compile_path_regex, compile_policy, sample_subject,
};
