# Core Principles

## Priority

Optimize in this order:

1. contractual correctness;
2. clarity;
3. measured performance.

An Accepted RFC or ADR outranks implementation convenience. When implementation pressure conflicts with one, stop the conflicting change and revise the governing document first if the product decision has intentionally changed.

## Behavior

- Make resolution deterministic from explicit inputs.
- Keep planning free of filesystem mutation.
- Make persistent mutations atomic, idempotent, and recoverable.
- Reject invalid, unknown, or corrupt state explicitly.
- Preserve the distinction between Intent, Environment, Activation, Durable Asset, cache, and Projection defined in [CONTEXT.md](../../CONTEXT.md).

## Scope

Implement the narrowest complete contract. Add extensibility after a concrete second use case appears. Prefer a small typed model over passthrough configuration.
