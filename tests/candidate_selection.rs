use std::str::FromStr;

use ghostty_wall::{
    codec::candidate_set::{candidate_set_digest, encode_canonical, fingerprint},
    domain::{CandidatePath, CandidateSet, CandidateSetDigest, ResolutionSeed},
    selection::{RandomSelectionError, random_v1_index, select_random_v1},
};

const RFC_0002_DIGEST: &str = "202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f";

#[test]
fn candidate_paths_enforce_rfc_0004_identity_rules() {
    for valid in [
        "tokyo/night.jpg",
        "Tokyo.jpg",
        "é.jpg",
        "e\u{301}.jpg",
        "folder:name/image.png",
    ] {
        assert_eq!(
            CandidatePath::from_str(valid)
                .expect("RFC-valid Candidate path must parse")
                .as_str(),
            valid
        );
    }

    for invalid in [
        "",
        "/absolute.jpg",
        "trailing/",
        "double//slash.jpg",
        "./image.jpg",
        "folder/../image.jpg",
        r"windows\image.jpg",
        "control/\u{7f}.jpg",
    ] {
        assert!(
            CandidatePath::from_str(invalid).is_err(),
            "{invalid:?} must be rejected"
        );
    }
}

#[test]
fn candidate_set_deduplicates_and_sorts_raw_utf8_bytes() {
    let set = CandidateSet::new(
        [
            "tokyo.jpg",
            "é.jpg",
            "Tokyo.jpg",
            "e\u{301}.jpg",
            "tokyo.jpg",
        ]
        .into_iter()
        .map(candidate),
    );

    let paths: Vec<_> = set.candidates().iter().map(CandidatePath::as_str).collect();
    assert_eq!(paths, ["Tokyo.jpg", "e\u{301}.jpg", "tokyo.jpg", "é.jpg"]);
}

#[test]
fn rfc_0004_empty_candidate_set_has_stable_canonical_json_and_fingerprint() {
    let set = CandidateSet::new([]);

    assert_eq!(
        encode_canonical(&set),
        br#"{"candidates":[],"schema_version":1}"#
    );
    assert_eq!(
        candidate_set_digest(&set).to_string(),
        "c654dd4ae81849f0b794d4a3e629fcadd0762969b76e1ad817745e02e8ac0280"
    );
    assert_eq!(
        fingerprint(&set).to_string(),
        "cset-v1-c654dd4ae81849f0b794d4a3e629fcadd0762969b76e1ad817745e02e8ac0280"
    );

    let seed = ResolutionSeed::from_bytes([0; 32]);
    assert_eq!(
        select_random_v1(&set, &seed),
        Err(RandomSelectionError::EmptyCandidateSet)
    );
}

#[test]
fn rfc_0004_non_empty_candidate_set_has_stable_canonical_json_and_fingerprint() {
    let set = CandidateSet::new(
        ["tokyo.jpg", "japan/night.jpg", "minimal/black.png"]
            .into_iter()
            .map(candidate),
    );

    assert_eq!(
        encode_canonical(&set),
        br#"{"candidates":["japan/night.jpg","minimal/black.png","tokyo.jpg"],"schema_version":1}"#
    );
    assert_eq!(
        candidate_set_digest(&set).to_string(),
        "e91de812a09b8d2daa3b7162a024f97cdbb8d182dde293ca357beff816e55548"
    );
    assert_eq!(
        fingerprint(&set).to_string(),
        "cset-v1-e91de812a09b8d2daa3b7162a024f97cdbb8d182dde293ca357beff816e55548"
    );
}

#[test]
fn rfc_0002_ordinary_random_v1_vector_selects_index_five() {
    let seed = ResolutionSeed::from_str(
        "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
    )
    .expect("RFC seed must parse");
    let digest = CandidateSetDigest::from_str(RFC_0002_DIGEST).expect("RFC digest must parse");

    assert_eq!(
        random_v1_index(&seed, digest, 10).expect("ordinary vector must select"),
        5
    );
}

#[test]
fn rfc_0002_rejection_vector_advances_to_counter_one() {
    let seed = ResolutionSeed::from_str(
        "010102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
    )
    .expect("RFC seed must parse");
    let digest = CandidateSetDigest::from_str(RFC_0002_DIGEST).expect("RFC digest must parse");

    assert_eq!(
        random_v1_index(&seed, digest, 9_223_372_036_854_775_809)
            .expect("rejection vector must select"),
        6_862_266_296_117_624_273
    );
}

#[test]
fn random_v1_selects_from_the_canonical_candidate_order() {
    let set = CandidateSet::new(["z.jpg", "a.jpg", "m.jpg"].into_iter().map(candidate));
    let seed = ResolutionSeed::from_bytes([42; 32]);

    let (index, selected) =
        select_random_v1(&set, &seed).expect("non-empty Candidate Set must select");

    assert_eq!(selected, &set.candidates()[index]);
}

fn candidate(value: &str) -> CandidatePath {
    CandidatePath::from_str(value).expect("test Candidate path must be valid")
}
