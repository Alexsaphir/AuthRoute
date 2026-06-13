# ADR-0002 — Per-route authorization custom resource

- **Status:** Proposed
- **Date:** 2026-06-13
- **Supersedes:** —
- **Companion:** [ADR-0001](0001-authroute-a-kubernetes-native-auth-gateway.md), [ADR-0004](0004-envoy-gateway-integration-mechanism.md)
- **Scope:** CRD surface — `v1alpha1`

## Context

[ADR-0001](0001-authroute-a-kubernetes-native-auth-gateway.md) commits AuthRoute
to expressing authorization intent as a custom resource attached to each route,
rather than a central config file. This ADR must decide the **shape of that CRD**:

- What is the resource called, and what is its scope (namespaced vs. cluster)?
- How does it select/attach to a route? Does it `targetRef` a Gateway API
  `HTTPRoute` (the way Envoy Gateway's `SecurityPolicy` does), reference a
  `Gateway`, or carry route-matching of its own?
- How is the core intent modeled — the **needs-auth toggle** and the set of
  **allowed groups and/or users**? Following ADR-0001's load-bearing principle,
  "public vs. requires-auth-and-these-principals" should be a typed, exhaustive
  enum so invalid combinations (e.g. "public *and* restricted to group X") are
  unrepresentable.
- How does this resource relate to Envoy Gateway's `SecurityPolicy` — is it a
  higher-level abstraction AuthRoute reconciles *into* a `SecurityPolicy`, or
  does it sit beside one? (Coordinate with [ADR-0004](0004-envoy-gateway-integration-mechanism.md).)
- What does `.status` report (attached? accepted by the gateway? conflicts?), and
  what happens when multiple policies target the same route?

## Decision

_To be decided._

## Consequences

_To be completed once the decision is made._
