use std::{fmt, str::FromStr};

use thiserror::Error;

/// Maximum JSON-safe, contiguous Activation sequence in v1.
pub const MAX_ACTIVATION_SEQUENCE: u64 = 9_007_199_254_740_991;

/// Local History identity derived solely from its nonzero sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActivationId(u64);

/// Invalid v1 Activation identity or sequence.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
#[error("Activation ID must be act-v1- plus 16 lowercase hex digits encoding sequence 1..=2^53-1")]
pub struct InvalidActivationId;

impl ActivationId {
    /// Constructs a canonical Activation ID from a validated sequence.
    pub fn new(sequence: u64) -> Result<Self, InvalidActivationId> {
        if (1..=MAX_ACTIVATION_SEQUENCE).contains(&sequence) {
            Ok(Self(sequence))
        } else {
            Err(InvalidActivationId)
        }
    }

    /// Returns the local History sequence encoded by this ID.
    pub const fn sequence(self) -> u64 {
        self.0
    }
}

impl FromStr for ActivationId {
    type Err = InvalidActivationId;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let digits = value.strip_prefix("act-v1-").ok_or(InvalidActivationId)?;
        if digits.len() != 16
            || !digits
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(InvalidActivationId);
        }
        Self::new(u64::from_str_radix(digits, 16).map_err(|_| InvalidActivationId)?)
    }
}

impl fmt::Display for ActivationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "act-v1-{:016x}", self.0)
    }
}
