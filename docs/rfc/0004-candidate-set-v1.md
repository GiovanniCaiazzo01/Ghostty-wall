# RFC 0004: Candidate Set v1

Status: Accepted
Date: 2026-09-23

## Purpose

This RFC defines Candidate identity, complete Candidate Set construction, Candidate Set identity, and consistent acquisition from GitHub and local-directory Sources.

A Candidate Set describes the selection space. Source revision and drift describe Source stability. Candidate content is represented later by a Durable Asset digest. These concerns MUST remain separate.

## Candidate identity

A Candidate identity is the path relative to the logical Source root.

Examples:

```text
tokyo/night.jpg
minimal/black.png
```

A Candidate identity:

- MUST be valid UTF-8;
- MUST use `/` as its separator;
- MUST be relative;
- MUST NOT begin or end with `/`;
- MUST NOT contain empty, `.`, or `..` segments;
- MUST NOT contain `\` or ASCII control characters;
- is case-sensitive;
- is compared without Unicode normalization.

Consequently, these identities are distinct:

```text
Tokyo.jpg
tokyo.jpg
é.jpg
é.jpg
```

A Candidate identity does not contain a Source identifier, absolute path, URL, commit, or asset digest.

## Eligibility

Source enumeration is always recursive in v1. There is no recursive configuration option.

Only regular files with one of these ASCII case-insensitive extensions are eligible:

```text
.png
.jpg
.jpeg
```

Extension determines discovery eligibility only. It does not establish media type or valid image content.

Symbolic links, directories, submodules, and non-regular entries are not Candidates.

An entry whose Candidate identity cannot be represented as valid UTF-8 is ignored and increments the enumeration diagnostic `skipped_non_utf8_entries`. Diagnostics do not participate in Candidate Set identity.

## Canonical Candidate Set

Construction is:

1. enumerate every eligible Candidate identity;
2. remove exact duplicate identities;
3. sort ascending by raw UTF-8 bytes.

Locale ordering, natural ordering, case folding, Unicode normalization, filesystem order, and remote API order MUST NOT affect the result.

The sorted array is the array consumed by RFC 0002.

An empty Candidate Set is a successful enumeration result. It has a fingerprint but cannot produce a random Selection.

## Candidate Set document

The canonical document is:

```json
{
  "schema_version": 1,
  "candidates": [
    "japan/night.jpg",
    "minimal/black.png",
    "tokyo.jpg"
  ]
}
```

`schema_version` MUST equal `1`. `candidates` MUST satisfy the canonical identity, uniqueness, and ordering rules above.

The document is canonicalized with RFC 8785 JCS:

```text
canonical = JCS(CandidateSetDocument)

candidate_set_digest = SHA-256(
    ASCII("ghostty-wall.candidate-set.v1")
    || 0x00
    || canonical
)

candidate_set_fingerprint =
    "cset-v1-" || lowercase_hex(candidate_set_digest)
```

The Candidate Set Digest is 32 raw bytes. The public Candidate Set Fingerprint MUST match:

```text
^cset-v1-[0-9a-f]{64}$
```

RFC 0002 consumes the raw Candidate Set Digest, not either textual representation.

The document describes ordered membership only. Candidate bytes, file metadata, Source identity, Source revision, diagnostics, and content hashes MUST NOT appear.

## Empty-set test vector

Canonical JCS bytes interpreted as UTF-8:

```text
{"candidates":[],"schema_version":1}
```

Expected digest:

```text
c654dd4ae81849f0b794d4a3e629fcadd0762969b76e1ad817745e02e8ac0280
```

Expected fingerprint:

```text
cset-v1-c654dd4ae81849f0b794d4a3e629fcadd0762969b76e1ad817745e02e8ac0280
```

Passing this Candidate Set to `random-v1` MUST return an `EmptyCandidateSet` error before division or modulo.

## Non-empty test vector

Input Candidate identities:

```text
japan/night.jpg
minimal/black.png
tokyo.jpg
```

Canonical JCS bytes interpreted as UTF-8:

```text
{"candidates":["japan/night.jpg","minimal/black.png","tokyo.jpg"],"schema_version":1}
```

Expected digest:

```text
e91de812a09b8d2daa3b7162a024f97cdbb8d182dde293ca357beff816e55548
```

Expected fingerprint:

```text
cset-v1-e91de812a09b8d2daa3b7162a024f97cdbb8d182dde293ca357beff816e55548
```

## GitHub Source resolution

GitHub resolution MUST:

1. resolve the configured repository and requested ref to one commit;
2. retain that exact commit for the complete invocation;
3. enumerate the logical Source root at that commit;
4. construct the complete Candidate Set;
5. perform Selection;
6. acquire the selected path from the same commit.

After ref resolution, no request in that invocation may address the movable branch or tag again.

Only Git tree modes `100644` and `100755` are regular-file candidates. Mode `120000` symlinks, mode `160000` submodules, directories, and other modes are excluded. The executable bit does not affect Candidate identity.

A truncated recursive Git Trees response is not a complete enumeration and MUST NOT be used to construct a Candidate Set. The adapter MUST either traverse trees non-recursively until enumeration is complete or return an explicit Source resolution error.

GitHub Source failures extend the RFC 0005 Error Response registry with these category `resolution` codes, each containing exactly `source_id` in addition to `category` and `code`:

```text
source.github-authentication-failed
source.github-rate-limited
source.github-unavailable
source.github-incomplete-tree
```

These errors MUST NOT contain requested refs, credentials, URLs, headers, response bodies, or transport details. `source.github-incomplete-tree` is emitted only when enumeration cannot prove complete membership.

The resolved commit is Source revision provenance. It does not participate in Candidate Set identity.

## Local-directory Source resolution

The configured local Source root may itself be a symbolic link. It is canonicalized once to an existing directory, and that canonical directory becomes the SourceRoot security boundary.

Below SourceRoot:

- enumeration is recursive;
- directory symlinks are not followed;
- file symlinks are not Candidates;
- Candidate access MUST remain beneath SourceRoot;
- no symbolic link may be resolved in any Candidate path component.

Selected local Candidates MUST be reopened relative to the canonical SourceRoot with semantics equivalent to Linux `RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS`. Protecting only the final path component is insufficient. The reopened entry MUST still be a regular file.

Local random resolution is:

```text
enumerate Candidate Set A
select Candidate
reopen safely beneath SourceRoot
read and stage selected bytes
validate and hash staged bytes
enumerate Candidate Set B
require fingerprint(A) == fingerprint(B)
```

A fingerprint mismatch means the Source changed during resolution and is an error. Ghostty Wall does not hash every Candidate's content to invent a local Source revision.

The staged selected bytes are the bytes validated, hashed, and later persisted by apply. Apply MUST NOT reopen the Source file after planning. Staged bytes are ephemeral execution resources and are not part of public Plan JSON.

## Asset acquisition

Acquisition validates the staged or downloaded bytes as a supported PNG or JPEG and derives media type from those bytes. A Candidate with eligible extension but invalid or unsupported content fails acquisition.

The SHA-256 digest of the exact acquired bytes identifies the Durable Asset. Changing bytes at the same Candidate path does not change Candidate Set identity, but it does change the resulting asset digest and may change the Environment.

## Direct path Selection

A Profile using `selection = "path"` does not require full Source enumeration or a Candidate Set Fingerprint.

The requested identity MUST pass Candidate path and extension eligibility rules. The resolver acquires it directly:

- from the resolved GitHub commit; or
- safely beneath the canonical local SourceRoot.

The acquired bytes remain subject to media validation and asset hashing.

## Plan observations

For random Selection, a Plan records:

- Source identity and kind;
- source-specific revision data when available;
- Candidate Set Fingerprint;
- Selection Algorithm and Resolution Seed;
- selected Candidate identity and index;
- acquired asset digest.

For direct path Selection, Candidate Set Fingerprint and selected index are absent.

Plan remains an informational invocation snapshot, not a persistent executable artifact.
