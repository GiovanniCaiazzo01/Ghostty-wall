# RFC 0002: Random Selection v1

Status: Accepted
Date: 2026-09-23

## Purpose

This RFC defines `random-v1`, the implementation-independent Selection Algorithm used when a Profile requests a random Candidate.

The algorithm is deterministic. Randomness enters only through an explicit Resolution Seed acquired before resolution.

## Inputs

`random-v1` consumes:

- `seed`: exactly 32 bytes;
- `candidate_set_digest`: exactly 32 raw bytes;
- `candidate_count`: the number of Candidates in the canonically ordered Candidate Set.

The Resolution Seed is serialized as exactly 64 lowercase hexadecimal characters.

The Candidate Set Digest is exposed publicly through the Candidate Set Fingerprint:

```text
cset-v1-<64 lowercase hexadecimal characters>
```

`random-v1` consumes the decoded 32-byte digest, never the hexadecimal suffix or complete fingerprint string.

`candidate_count` MUST be greater than zero and no greater than `2^64`. An empty Candidate Set is an error.

Candidate identity, canonical ordering, and Candidate Set fingerprinting are defined by the Source contract. They are not redefined here.

## Seed acquisition

- `apply PROFILE` obtains 32 bytes from the operating system random-number generator when resolution requires randomness.
- `apply PROFILE --seed HEX` uses the supplied seed.
- `plan PROFILE` requires `--seed HEX` when resolution requires randomness.
- Supplying a seed to a resolution that does not require randomness is an error.

The seed MUST appear in the resulting Plan and Activation provenance. It MUST NOT appear in an Environment Manifest or affect Environment identity.

## Digest blocks

Let:

```text
domain = ASCII("ghostty-wall.random-v1") || 0x00
counter = an unsigned 64-bit integer starting at 0
counter_bytes = counter encoded as 8-byte big-endian
```

For each counter value, derive:

```text
block = SHA-256(
    domain
    || seed
    || candidate_set_digest
    || counter_bytes
)
```

Interpret the first eight bytes of `block` as an unsigned big-endian 64-bit integer named `value`.

Implementations MUST NOT substitute a language or library pseudo-random number generator.

## Unbiased index selection

Let:

```text
N = candidate_count
limit = floor(2^64 / N) * N
```

The arithmetic definition of `limit` uses an integer type capable of representing `2^64`.

For counters beginning at zero:

1. derive `value`;
2. reject the value when `value >= limit`;
3. otherwise return `index = value mod N`;
4. if rejected, increment the counter and derive the next block.

When `N` divides `2^64`, `limit` is `2^64` and every 64-bit value is accepted.

Counter overflow is an error. An implementation MUST NOT fall back to biased modulo selection.

The selected Candidate is:

```text
canonical_candidates[index]
```

## Reproducibility contract

The same:

- Selection Algorithm version;
- Resolution Seed;
- canonical Candidate Set;

MUST produce the same Selection.

Profile and Source definitions produce the Candidate Set but are not inputs to `random-v1` itself.

The same seed against a different Candidate Set is not expected to select the same Candidate.

## Source drift

A Plan MUST record:

- `selection_algorithm = "random-v1"`;
- the Resolution Seed;
- the Candidate Set Fingerprint;
- the selected Candidate identity;
- the selected index;
- the acquired asset digest once known;
- source-specific snapshot data, such as a resolved Git commit.

A Plan is an informational snapshot. It is not a persistent executable artifact.

`apply` builds and executes a new Plan. Reusing a seed reproduces a Selection only while the canonical Candidate Set is unchanged. Source drift MUST be surfaced through changed snapshot data or fingerprint, not hidden.

## Test vector 1: ordinary selection

Inputs:

```text
seed =
000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f

candidate_set_digest =
202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f

candidate_count = 10
counter = 0
```

Expected block:

```text
0d1a3dd7a5c06ea39a87f156d659d7b7f3c9832ddeb89f314488428b4fa0a57a
```

Expected values:

```text
value = 944135068295655075
limit = 18446744073709551610
index = 5
```

## Test vector 2: rejection

This vector exercises rejection sampling independently of Candidate Set allocation.

Inputs:

```text
seed =
010102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f

candidate_set_digest =
202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f

candidate_count = 9223372036854775809
limit = 9223372036854775809
```

Counter zero:

```text
block =
b3e95e501047793566dcf67107143e606ad791012a6cf2627d40721452af7f34

value = 12963996700326197557
result = rejected
```

Counter one:

```text
block =
5f3baad6a639fdd178e4b707684656db9b3016429b29a5cca97b68a8fe2fb097

value = 6862266296117624273
index = 6862266296117624273
```

An implementation MUST pass both vectors before emitting `selection_algorithm = "random-v1"`.
