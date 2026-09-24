//! Pure, versioned Candidate Selection Algorithms.

use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    codec::candidate_set::candidate_set_digest,
    domain::{CandidatePath, CandidateSet, CandidateSetDigest, ResolutionSeed},
};

const RANDOM_V1_DOMAIN: &[u8] = b"ghostty-wall.random-v1\0";
const TWO_TO_64: u128 = 1_u128 << 64;

/// Failure to select an index with `random-v1`.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum RandomSelectionError {
    /// Selection was requested from a successfully enumerated empty set.
    #[error("random-v1 cannot select from an empty Candidate Set")]
    EmptyCandidateSet,

    /// The supplied count exceeded the RFC 0002 maximum of 2^64.
    #[error("random-v1 Candidate count {candidate_count} exceeds 2^64")]
    CandidateCountTooLarge {
        /// Rejected Candidate count.
        candidate_count: u128,
    },

    /// Every counter value was exhausted without an accepted sample.
    #[error("random-v1 counter overflowed before selecting an index")]
    CounterOverflow,
}

/// Selects an unbiased canonical index using RFC 0002 `random-v1`.
///
/// `candidate_count` may be at most 2^64. This lower-level interface accepts
/// counts that cannot be represented by an in-memory [`CandidateSet`], allowing
/// implementations to verify the complete cross-platform algorithm contract.
pub fn random_v1_index(
    seed: &ResolutionSeed,
    candidate_set_digest: CandidateSetDigest,
    candidate_count: u128,
) -> Result<u64, RandomSelectionError> {
    if candidate_count == 0 {
        return Err(RandomSelectionError::EmptyCandidateSet);
    }
    if candidate_count > TWO_TO_64 {
        return Err(RandomSelectionError::CandidateCountTooLarge { candidate_count });
    }

    let limit = (TWO_TO_64 / candidate_count) * candidate_count;
    let mut counter = 0_u64;

    loop {
        let value = random_v1_value(seed, candidate_set_digest, counter);

        // Modulo is unbiased only over a prefix whose length is divisible by
        // the Candidate count; rejected tail values consume the next block.
        if u128::from(value) < limit {
            return Ok((u128::from(value) % candidate_count) as u64);
        }

        counter = counter
            .checked_add(1)
            .ok_or(RandomSelectionError::CounterOverflow)?;
    }
}

/// Selects a Candidate from canonical order using RFC 0002 `random-v1`.
///
/// The returned index and borrowed path refer to the supplied Candidate Set.
/// Its digest is derived internally so callers cannot pair a set with an
/// unrelated fingerprint.
pub fn select_random_v1<'a>(
    candidate_set: &'a CandidateSet,
    seed: &ResolutionSeed,
) -> Result<(usize, &'a CandidatePath), RandomSelectionError> {
    if candidate_set.is_empty() {
        return Err(RandomSelectionError::EmptyCandidateSet);
    }

    let index = random_v1_index(
        seed,
        candidate_set_digest(candidate_set),
        candidate_set.len() as u128,
    )?;
    let index = usize::try_from(index)
        .expect("random-v1 index is less than a Candidate Set length representable by usize");

    Ok((index, &candidate_set.candidates()[index]))
}

fn random_v1_value(
    seed: &ResolutionSeed,
    candidate_set_digest: CandidateSetDigest,
    counter: u64,
) -> u64 {
    let mut hasher = Sha256::new();
    hasher.update(RANDOM_V1_DOMAIN);
    hasher.update(seed.as_bytes());
    hasher.update(candidate_set_digest.as_bytes());
    hasher.update(counter.to_be_bytes());
    let block = hasher.finalize();
    let mut value_bytes = [0_u8; 8];
    value_bytes.copy_from_slice(&block[..8]);
    u64::from_be_bytes(value_bytes)
}
