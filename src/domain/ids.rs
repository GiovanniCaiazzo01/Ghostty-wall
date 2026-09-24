use std::{fmt, str::FromStr};

use super::ValidationError;

/// A SHA-256 digest rendered as 64 lowercase hexadecimal characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Sha256Digest([u8; 32]);

impl Sha256Digest {
    /// Constructs a digest from its raw bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the raw 32-byte digest consumed by domain-separated algorithms.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl FromStr for Sha256Digest {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        parse_lower_hex::<32>(value)
            .map(Self)
            .ok_or(ValidationError::InvalidSha256Digest)
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_lower_hex(formatter, &self.0)
    }
}

/// The versioned content identity of an Environment Manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EnvironmentId(Sha256Digest);

impl EnvironmentId {
    /// Wraps a digest calculated according to RFC 0001.
    pub const fn from_digest(digest: Sha256Digest) -> Self {
        Self(digest)
    }

    /// Returns the underlying SHA-256 digest.
    pub const fn digest(&self) -> Sha256Digest {
        self.0
    }
}

impl FromStr for EnvironmentId {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let digest = value
            .strip_prefix("env-v1-")
            .ok_or(ValidationError::InvalidEnvironmentId)?
            .parse()
            .map_err(|_| ValidationError::InvalidEnvironmentId)?;

        Ok(Self(digest))
    }
}

impl fmt::Display for EnvironmentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "env-v1-{}", self.0)
    }
}

pub(super) fn parse_lower_hex<const N: usize>(value: &str) -> Option<[u8; N]> {
    let bytes = value.as_bytes();
    if bytes.len() != N * 2 {
        return None;
    }

    let mut decoded = [0_u8; N];
    for index in 0..N {
        let high = lower_hex_value(bytes[index * 2])?;
        let low = lower_hex_value(bytes[index * 2 + 1])?;
        decoded[index] = (high << 4) | low;
    }

    Some(decoded)
}

fn lower_hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn write_lower_hex(formatter: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    for byte in bytes {
        write!(formatter, "{byte:02x}")?;
    }
    Ok(())
}
