# Architecture

## Authority

RFCs and ADRs are authoritative. Change a governing document before intentionally changing its invariant.

- [RFC 0001](../rfc/0001-environment-manifest-v1.md) defines Environment identity and Projection.
- [RFC 0002](../rfc/0002-random-selection-v1.md) defines deterministic random Selection.
- [RFC 0003](../rfc/0003-intent-toml-v1.md) defines authoritative TOML intent.
- [RFC 0004](../rfc/0004-candidate-set-v1.md) defines Candidate discovery, identity, and Source consistency.
- [RFC 0005](../rfc/0005-plan-json-and-errors-v1.md) defines public planning, diagnostics, and error JSON.
- [RFC 0006](../rfc/0006-activation-and-history-v1.md) defines durable apply commits and local History.
- [RFC 0007](../rfc/0007-recovery-and-reconciliation-v1.md) defines synchronized inspection and safe Projection reconciliation.
- [RFC 0008](../rfc/0008-managed-layout-and-init-v1.md) defines Managed Root layout, initialization, migration, and uninstall.
- [ADR 0001](../adr/0001-materialize-a-flat-ghostty-configuration.md) requires a flat managed Ghostty Projection.
- [ADR 0002](../adr/0002-store-immutable-environments-and-durable-assets.md) requires immutable Environments and durable content-addressed assets.

## Boundaries

- Intent parsing validates syntax; domain construction validates semantics.
- Resolution transforms validated Intent and explicit observations into a Plan.
- Apply executes a freshly created Plan through adapters.
- Persistence owns atomic records and crash consistency.
- Projection converts an Environment into machine-local Ghostty configuration.
- Platform adapters reload or probe Ghostty without leaking platform behavior into the core.
- TUI and CLI invoke the same application services and contain no domain logic.

## Ownership

Ghostty Wall owns one include entry and its managed directory. It does not become a general Ghostty configuration manager. Generated output is replaceable; Environment records, Activation history, and referenced Durable Assets are durable.
