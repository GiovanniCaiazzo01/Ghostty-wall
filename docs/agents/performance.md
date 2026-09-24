# Performance

Correctness and clarity precede optimization.

- Measure the relevant path before adding concurrency, SIMD, caching, or allocation complexity.
- Keep deterministic output independent of task scheduling and completion order.
- Bound network responses, decoded image dimensions, memory use, and concurrent work.
- Stream or stage large assets when that materially lowers peak memory without weakening atomicity.
- Cache only reconstructible data. A wallpaper referenced by an Environment belongs in the Durable Asset Store, not cache.
- Keep cancellation and partial-failure behavior explicit for concurrent work.

Record the benchmark or profile that justifies non-obvious optimization.
