# Ghostty Wall

Ghostty Wall manages reproducible visual customization states for Ghostty.

## Language

**Profile**:
A named, persistent, declarative recipe from which an Environment can be produced.
_Avoid_: Scene, Environment

**Environment**:
An immutable, portable snapshot of only the managed Projection produced from a Profile at a specific moment.
_Avoid_: Scene, Profile

**Wallpaper Source**:
A configured origin from which Ghostty Wall discovers wallpaper candidates.
_Avoid_: Repository, collection

**Candidate**:
A selectable wallpaper reference discovered from a Wallpaper Source.
_Avoid_: Wallpaper, Durable Asset

**Candidate Set**:
The canonically ordered Candidates produced by resolving a Wallpaper Source.
_Avoid_: Collection, cache

**Candidate Set Digest**:
The raw 32-byte digest of a canonical Candidate Set document.
_Avoid_: Candidate Set Fingerprint, asset digest

**Candidate Set Fingerprint**:
The versioned textual identity encoding of a Candidate Set Digest.
_Avoid_: Candidate Set Digest, Environment ID

**Resolution Seed**:
The explicit 32-byte entropy input used by a Selection Algorithm.
_Avoid_: random state, Environment metadata

**Selection Algorithm**:
A versioned deterministic procedure that chooses one Candidate from a Candidate Set using a Resolution Seed.
_Avoid_: random-number generator

**Selection**:
The deterministic choice of one Candidate from a Candidate Set using a Selection Algorithm and Resolution Seed.
_Avoid_: Wallpaper, Activation

**Wallpaper**:
An image discovered from a Wallpaper Source before it becomes durable.
_Avoid_: Source, Durable Asset

**Durable Asset**:
Wallpaper content retained by its content hash so an applied Environment can be replayed without its original Source.
_Avoid_: Cache, Wallpaper Source

**Environment Manifest**:
The canonical, immutable description of a fully resolved Environment. Its semantic content determines the Environment identity.
_Avoid_: Plan, Profile, metadata

**Plan**:
A deterministic description of the operations required to produce and apply an Environment.
_Avoid_: Environment, Environment Manifest

**Diagnostic**:
A structured non-fatal observation from one resolution invocation.
_Avoid_: Error, log message

**Error Response**:
The machine-readable description of the single primary failure that prevented a complete result.
_Avoid_: Diagnostic, partial Plan

**Activation**:
An immutable historical event recording that an Environment became current, including its cause and provenance.
_Avoid_: Environment, history index

**History**:
The append-only local sequence of committed Activations.
_Avoid_: Environment list, current state file

**History Cursor**:
The persisted sequence position used for backward navigation independently of Activation genealogy.
_Avoid_: Activation ID, array index

**Recovery Inspection**:
A synchronized read-only comparison of durable state, Projection, and Ghostty integration.
_Avoid_: Reconciliation, repair

**Reconciliation**:
A synchronized mutation that restores derived Projection state from the latest committed Activation.
_Avoid_: durable-state repair, Activation

**Managed Root**:
The single platform-specific directory owned by Ghostty Wall for Intent, durable state, derived state, and coordination.
_Avoid_: Ghostty root config, cache directory

**Integration Hook**:
The single semantic `config-file` directive connecting Ghostty's effective root configuration to the managed Projection.
_Avoid_: Projection, managed config

**Projection**:
A disposable, machine-local representation of an Environment for an external consumer, such as Ghostty's active configuration.
_Avoid_: State, Environment
