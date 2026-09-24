# Code Comments

Comments are part of the implementation.

Their purpose is not to narrate syntax. They should preserve information
that cannot be recovered cheaply from the local code, or reduce the
cognitive load required to understand a complex section.

A code change is incomplete when it changes an invariant, contract, or
reasoning documented by a nearby comment without updating that comment.

## 1. Function and API comments

Use comments or Rust doc comments to let a caller understand an interface
without reading its implementation.

Document:

- the abstraction or contract;
- important inputs and outputs;
- failure conditions;
- externally observable side effects;
- invariants a caller must preserve.

Public APIs SHOULD have useful documentation close to their definitions.

Do not document implementation details that callers do not need.

## 2. Design comments

Use design comments near a module or substantial implementation when the
local code cannot communicate the design by itself.

A design comment SHOULD explain:

- the overall approach;
- important invariants;
- why the approach was selected;
- relevant alternatives that were rejected;
- non-obvious tradeoffs.

Do not duplicate an RFC or ADR.

When an RFC or ADR already owns the decision, link to it and explain only
the local implementation consequences.

## 3. Why comments

Use a why comment when the code is easy to understand mechanically but the
reason for doing it that way is not obvious.

Typical cases include:

- ordering constraints;
- crash-safety requirements;
- compatibility requirements;
- security boundaries;
- performance decisions;
- intentionally surprising code;
- regression prevention.

A good why comment should make an apparently tempting refactor look
obviously unsafe when it would violate an invariant.

## 4. Teacher comments

Use teacher comments when understanding the implementation requires domain
knowledge that a competent Rust developer may not reasonably have loaded in
working memory.

Examples for this project include:

- rejection sampling;
- JCS canonicalization;
- content-addressed storage;
- filesystem durability and fsync semantics;
- no-follow path resolution.

Explain only enough theory to understand the code.

Prefer links to authoritative documentation when a full treatment would be
too large.

## 5. Checklist comments

Use checklist comments when changing one location requires coordinated
changes elsewhere and that coupling cannot reasonably be expressed by the
type system, tests, or architecture.

Example:

    // When adding a new Environment Manifest field:
    // - update RFC 0001;
    // - update canonicalization tests;
    // - update the projector;
    // - update corruption validation.

Prefer eliminating the coupling over documenting it when practical.

A checklist comment is a defensive tool, not a substitute for architecture.

## 6. Guide comments

Guide comments may divide a long or cognitively dense procedure into
meaningful phases.

They are appropriate when they reduce the amount of state the reader must
keep in mind.

For example:

    // Validate committed durable state.

    ...

    // Reconcile the derived Projection.

    ...

    // Publish the new Activation.

Do not add guide comments mechanically to every small block.

## 7. Trivial comments

Do not write comments that merely translate the next statement into English.

Bad:

    // Increment the sequence.
    sequence += 1;

The comment must reduce cognitive load or provide information not already
obvious from the code.

## 8. Debt comments

Avoid TODO, FIXME, XXX, and "temporary hack" comments as a substitute for
tracking architectural or product work.

Prefer:

- an issue;
- an RFC;
- an ADR;
- an explicit backlog document.

A temporary debt comment is acceptable only when keeping the information
adjacent to the code is materially useful.

When possible it SHOULD reference the corresponding tracked work and explain
the condition that makes the debt relevant.

## 9. Backup comments

Never keep old implementations as commented-out code.

Version control is the backup.

If the replacement is not trustworthy enough to remove the previous
implementation, the change is not ready.

## Comments as a design check

Writing a comment can expose an underspecified or accidental behavior.

If an invariant, failure mode, or algorithm cannot be explained clearly,
do not hide that uncertainty behind a vague comment.

Revisit the design, test, RFC, or implementation until the behavior can be
stated precisely.

## Project-specific expectations

Ghostty-wall contains several areas where comments are especially valuable.

Use why/design comments around:

- durable commit points;
- filesystem no-follow guarantees;
- atomic replacement and fsync ordering;
- history cursor semantics;
- corruption versus recoverable Projection drift;
- domain-separated hashes and canonicalization;
- compatibility behavior required by accepted RFCs.

Use teacher comments where algorithms such as `random-v1` would otherwise
require the reader to reconstruct theory from the implementation.

Do not reproduce normative RFC text inside source comments. Point to the RFC
and explain the local consequence instead.

## Review rule

During review, ask:

1. Does this comment contain information the code cannot communicate cheaply?
2. Does it lower cognitive load?
3. Is it still true?
4. Would a future maintainer know what must remain invariant?
5. Is this information better owned by an RFC, ADR, test, or issue?

If the answer indicates the comment adds no value, remove it.
