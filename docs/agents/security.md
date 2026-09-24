# Security

Treat Profiles, Source metadata, remote responses, theme files, image bytes, persisted state, and filesystem contents as untrusted input.

- Validate paths before access; prevent traversal outside the resolved Source root.
- Keep remote repository refs and paths as data, never shell fragments.
- Bound downloads, redirects, image dimensions, decode work, and decompressed size.
- Verify asset bytes against their recorded digest before replay.
- Write temporary files with restrictive permissions and publish them through atomic replacement.
- Avoid following links across an ownership boundary unless a governing Source contract explicitly permits it.
- Read credentials from supported environment or platform facilities. Never persist tokens in Intent, Plan, Activation, logs, or diagnostics.
- Redact URLs or headers that may contain credentials.

Shell execution is an adapter of last resort. Pass arguments without interpolation and expose the exact external action in diagnostics.
