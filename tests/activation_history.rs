use std::str::FromStr;

use ghostty_wall::domain::ActivationId;

#[test]
fn activation_id_uses_fixed_width_lowercase_hex_sequence() {
    let id = ActivationId::new(42).unwrap();
    assert_eq!(id.to_string(), "act-v1-000000000000002a");
    assert_eq!(id.sequence(), 42);
    assert_eq!(
        ActivationId::from_str("act-v1-000000000000002a").unwrap(),
        id
    );
    assert_eq!(
        ActivationId::new(9_007_199_254_740_991).unwrap().sequence(),
        9_007_199_254_740_991
    );
}

#[test]
fn activation_id_rejects_zero_oversize_and_noncanonical_text() {
    assert!(ActivationId::new(0).is_err());
    assert!(ActivationId::new(9_007_199_254_740_992).is_err());
    for value in [
        "act-v1-0000000000000000",
        "act-v1-2a",
        "act-v1-000000000000002A",
        "act-v2-000000000000002a",
        "act-v1-002a000000000000",
    ] {
        assert!(ActivationId::from_str(value).is_err(), "{value}");
    }
}
