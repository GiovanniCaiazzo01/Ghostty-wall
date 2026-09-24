# Materialize a flat Ghostty configuration

Ghostty Wall materializes each active Environment as one complete `current.ghostty` file instead of constructing a nested managed include graph. Keeping Ghostty's precedence semantics at the single root include makes verification and deterministic replay simpler, removes runtime dependencies on generated fragments, and leaves a disposable projection with a clear ownership boundary.
