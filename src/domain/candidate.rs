use std::{fmt, str::FromStr};

use super::{ValidationError, ids::parse_lower_hex};

/// A validated Candidate identity relative to its logical Source root.
///
/// The path uses `/` separators and preserves its UTF-8 bytes exactly; it is
/// neither case-folded nor Unicode-normalized.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CandidatePath(String);

impl CandidatePath {
    /// Returns the exact validated path text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for CandidatePath {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() {
            return Err(ValidationError::EmptyCandidatePath);
        }
        if value.starts_with('/') {
            return Err(ValidationError::AbsoluteCandidatePath);
        }
        if value.contains('\\') {
            return Err(ValidationError::CandidatePathContainsBackslash);
        }
        if value.bytes().any(|byte| byte.is_ascii_control()) {
            return Err(ValidationError::CandidatePathContainsControl);
        }
        if value
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        {
            return Err(ValidationError::InvalidCandidatePathSegment);
        }

        Ok(Self(value.to_owned()))
    }
}

impl fmt::Display for CandidatePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Canonically ordered, duplicate-free Candidate identities.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CandidateSet(Vec<CandidatePath>);

impl CandidateSet {
    /// Canonicalizes validated Candidates by exact identity.
    ///
    /// Ordering is ascending by raw UTF-8 bytes, with exact duplicates removed.
    pub fn new(candidates: impl IntoIterator<Item = CandidatePath>) -> Self {
        let mut candidates: Vec<_> = candidates.into_iter().collect();
        candidates.sort_unstable();
        candidates.dedup();
        Self(candidates)
    }

    /// Returns Candidates in the canonical order consumed by Selection Algorithms.
    pub fn candidates(&self) -> &[CandidatePath] {
        &self.0
    }

    /// Returns the number of canonical Candidates.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether successful enumeration found no Candidates.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The raw 32-byte digest of a canonical Candidate Set document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CandidateSetDigest([u8; 32]);

impl CandidateSetDigest {
    /// Constructs a Candidate Set Digest from raw SHA-256 output bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the raw bytes consumed by Selection Algorithms.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl FromStr for CandidateSetDigest {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        parse_lower_hex(value)
            .map(Self)
            .ok_or(ValidationError::InvalidCandidateSetDigest)
    }
}

impl fmt::Display for CandidateSetDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_lower_hex(formatter, &self.0)
    }
}

/// The versioned textual identity of a Candidate Set Digest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CandidateSetFingerprint(CandidateSetDigest);

impl CandidateSetFingerprint {
    /// Wraps a digest calculated from an RFC 0004 canonical document.
    pub const fn from_digest(digest: CandidateSetDigest) -> Self {
        Self(digest)
    }

    /// Returns the raw digest represented by this fingerprint.
    pub const fn digest(self) -> CandidateSetDigest {
        self.0
    }
}

impl FromStr for CandidateSetFingerprint {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let digest = value
            .strip_prefix("cset-v1-")
            .ok_or(ValidationError::InvalidCandidateSetFingerprint)?
            .parse()
            .map_err(|_| ValidationError::InvalidCandidateSetFingerprint)?;

        Ok(Self(digest))
    }
}

impl fmt::Display for CandidateSetFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "cset-v1-{}", self.0)
    }
}

/// The explicit 32-byte entropy input to a Selection Algorithm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResolutionSeed([u8; 32]);

impl ResolutionSeed {
    /// Constructs a Resolution Seed from exactly 32 bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the exact entropy bytes consumed by a Selection Algorithm.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl FromStr for ResolutionSeed {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        parse_lower_hex(value)
            .map(Self)
            .ok_or(ValidationError::InvalidResolutionSeed)
    }
}

impl fmt::Display for ResolutionSeed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_lower_hex(formatter, &self.0)
    }
}

fn write_lower_hex(formatter: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    for byte in bytes {
        write!(formatter, "{byte:02x}")?;
    }
    Ok(())
}
