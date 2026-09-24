use thiserror::Error;

/// A violation encountered while constructing a validated domain value.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ValidationError {
    /// A Candidate path was empty.
    #[error("Candidate path must not be empty")]
    EmptyCandidatePath,

    /// A Candidate path was absolute rather than relative to its Source root.
    #[error("Candidate path must be relative")]
    AbsoluteCandidatePath,

    /// A Candidate path contained an empty, current-directory, or parent segment.
    #[error("Candidate path must not contain empty, '.', or '..' segments")]
    InvalidCandidatePathSegment,

    /// A Candidate path used a platform-specific backslash separator.
    #[error("Candidate path must use '/' separators and must not contain '\\'")]
    CandidatePathContainsBackslash,

    /// A Candidate path contained an ASCII control character.
    #[error("Candidate path must not contain ASCII control characters")]
    CandidatePathContainsControl,

    /// A Candidate Set Digest was not exactly 32 bytes of lowercase hexadecimal text.
    #[error("Candidate Set Digest must contain exactly 64 lowercase hexadecimal characters")]
    InvalidCandidateSetDigest,

    /// A Candidate Set Fingerprint had the wrong version prefix or digest.
    #[error("Candidate Set Fingerprint must match cset-v1-<64 lowercase hexadecimal characters>")]
    InvalidCandidateSetFingerprint,

    /// A Resolution Seed was not exactly 32 bytes of lowercase hexadecimal text.
    #[error("Resolution Seed must contain exactly 64 lowercase hexadecimal characters")]
    InvalidResolutionSeed,

    /// A Source or Profile identifier violated RFC 0003 slug rules.
    #[error("intent identifier must match ^[a-z0-9]+(?:-[a-z0-9]+)*$ and be 1..=64 bytes")]
    InvalidIntentId,

    /// A Source path violated RFC 0003 relative slash-path rules.
    #[error(
        "Source path must be relative slash-separated path without '.', '..', empty segments, or backslashes"
    )]
    InvalidSourcePath,

    /// A digest was not exactly 32 bytes of lowercase hexadecimal text.
    #[error("SHA-256 digest must contain exactly 64 lowercase hexadecimal characters")]
    InvalidSha256Digest,

    /// An Environment ID had the wrong version prefix or digest.
    #[error("environment ID must match env-v1-<64 lowercase hexadecimal characters>")]
    InvalidEnvironmentId,

    /// A color was not six lowercase hexadecimal sRGB digits.
    #[error("color must contain exactly 6 lowercase hexadecimal characters")]
    InvalidColor,

    /// A closed string enum received an unknown value.
    #[error("invalid {field} value {value:?}")]
    InvalidEnumValue {
        /// Name of the field whose closed enum was violated.
        field: &'static str,
        /// Rejected textual value.
        value: String,
    },

    /// The Manifest uses a schema version unsupported by this build.
    #[error("unsupported Environment Manifest schema version {found}")]
    UnsupportedManifestSchema {
        /// Version read from the Manifest.
        found: u64,
    },

    /// A managed color palette did not contain exactly 16 ANSI colors.
    #[error("palette must contain exactly 16 colors, found {found}")]
    InvalidPaletteLength {
        /// Number of supplied palette entries.
        found: usize,
    },

    /// A present terminal section managed no terminal property.
    #[error("terminal section must contain at least one managed property")]
    EmptyTerminal,

    /// Font size fell outside RFC 0001's fixed-point range.
    #[error("font size {value} millipoints is outside 1000..=1000000")]
    FontSizeOutOfRange {
        /// Rejected millipoint value.
        value: u64,
    },

    /// Opacity fell outside RFC 0001's fixed-point range.
    #[error("opacity {value} millionths is outside 0..=1000000")]
    OpacityOutOfRange {
        /// Rejected millionths value.
        value: u64,
    },

    /// Blur intensity did not fit Ghostty Wall's v1 byte range.
    #[error("background blur intensity {value} is outside 0..=255")]
    BackgroundBlurOutOfRange {
        /// Rejected blur intensity.
        value: u64,
    },
}
