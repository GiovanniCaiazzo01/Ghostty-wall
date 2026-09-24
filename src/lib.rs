//! Contract-first core for Ghostty Wall's visual Environment manager.

#![warn(missing_docs)]

/// Encoding and decoding of public and persisted protocols.
pub mod codec;
/// Validated domain types defined by the accepted RFCs.
pub mod domain;
/// Read-only validated local History inspection.
pub mod history;
/// Managed Root initialization.
pub mod init;
/// Read-only planning.
pub mod plan;
/// Read-only synchronized Recovery Inspection.
pub mod recovery;
/// Pure, versioned Candidate Selection Algorithms.
pub mod selection;
