//! Contract-first core for Ghostty Wall's visual Environment manager.

#![warn(missing_docs)]

/// Durable Profile apply and History replay.
pub mod apply;
/// Process command-line interface.
pub mod cli;
/// Encoding and decoding of public and persisted protocols.
pub mod codec;
/// Validated domain types defined by the accepted RFCs.
pub mod domain;
/// Commit-pinned GitHub API adapter boundary.
pub mod github;
/// Read-only validated local History inspection.
pub mod history;
/// Managed Root initialization.
pub mod init;
/// Installation diagnosis, legacy migration, and safe uninstall.
pub mod lifecycle;
mod palette;
/// Read-only planning.
pub mod plan;
/// Read-only synchronized Recovery Inspection.
pub mod recovery;
/// Best-effort Ghostty runtime reload adapters.
pub mod runtime;
/// Pure, versioned Candidate Selection Algorithms.
pub mod selection;
/// Terminal browser state machine and Ghostty image preview.
pub mod terminal_browser;
/// Read-only named Ghostty theme resolution.
pub mod theme;
