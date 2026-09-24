# Rust Conventions

Apply this document after the Rust rewrite begins.

## Types

- Represent validated identifiers, digests, colors, fixed-point values, and paths with dedicated types.
- Model closed domain choices with exhaustive enums and tagged variants.
- Keep unvalidated input types separate from validated domain types.
- Preserve absence as unmanaged; do not fill omitted values with Ghostty defaults.
- Parse contractual decimals lexically and convert them directly to fixed-point integers.

## APIs

- Keep resolution functions pure over explicit inputs.
- Keep filesystem, network, clock, entropy, and Ghostty integration behind narrow adapters.
- Prefer borrowing over cloning; clone only across an intentional ownership boundary.
- Keep public interfaces smaller than their implementations.

## Failures

- Return typed errors from fallible production paths.
- Reserve `unwrap`, `expect`, and deliberate panics for tests or states proven unreachable by construction.
- Reject unknown persisted fields and enum variants.

## Style

Use `rustfmt` defaults and Clippy-clean idiomatic Rust. Let names use the domain vocabulary from [CONTEXT.md](../../CONTEXT.md).
