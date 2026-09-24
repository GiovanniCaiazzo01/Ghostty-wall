# Dependencies

Add a dependency only when it owns a substantial, well-defined problem better than a small local implementation.

Before adding one, check:

- maintenance and release activity;
- license compatibility;
- transitive dependency and feature cost;
- platform support;
- security history;
- deterministic behavior required by RFCs.

Use the package manager to select and record versions. Disable unnecessary default features. Keep dependency types behind project-owned interfaces when they would otherwise leak into the domain model.

Cryptographic primitives, image decoders, HTTP, TOML, JCS, terminal rendering, and OS integration require established implementations or a documented reason to do otherwise. RFC-defined composition around those primitives remains project code.
