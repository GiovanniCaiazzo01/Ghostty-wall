use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::domain::{CandidateSet, CandidateSetDigest, CandidateSetFingerprint};

// RFC 0004 fixes these exact bytes so Candidate Set digests cannot be confused
// with hashes from another Ghostty Wall protocol.
const CANDIDATE_SET_DOMAIN: &[u8] = b"ghostty-wall.candidate-set.v1\0";

/// Encodes a validated Candidate Set as its RFC 8785 JCS document.
pub fn encode_canonical(candidate_set: &CandidateSet) -> Vec<u8> {
    let document = CandidateSetDocument {
        schema_version: 1,
        candidates: candidate_set
            .candidates()
            .iter()
            .map(|candidate| candidate.as_str())
            .collect(),
    };

    // This DTO contains only a fixed integer and validated strings, so serde's
    // data model has no fallible representation case.
    serde_jcs::to_vec(&document)
        .expect("a validated Candidate Set must always have a JCS representation")
}

/// Derives the raw, domain-separated RFC 0004 Candidate Set Digest.
pub fn candidate_set_digest(candidate_set: &CandidateSet) -> CandidateSetDigest {
    let mut hasher = Sha256::new();
    hasher.update(CANDIDATE_SET_DOMAIN);
    hasher.update(encode_canonical(candidate_set));

    CandidateSetDigest::from_bytes(hasher.finalize().into())
}

/// Derives the versioned textual fingerprint of a Candidate Set.
pub fn fingerprint(candidate_set: &CandidateSet) -> CandidateSetFingerprint {
    CandidateSetFingerprint::from_digest(candidate_set_digest(candidate_set))
}

#[derive(Serialize)]
struct CandidateSetDocument<'a> {
    schema_version: u64,
    candidates: Vec<&'a str>,
}
