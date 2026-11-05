use cadcraft_labels::template::{Owner, decode_owner, encode_owner};

#[test]
fn ownership_chunks_preserve_unicode_and_reject_unknown_versions() {
    let owner = Owner { version: 1, template: "測".repeat(20), source: "F:segment:123".into(), role: "label".into() };
    let chunks = encode_owner(&owner).unwrap();
    assert!(chunks.iter().all(|s| s.len() <= 240));
    assert_eq!(decode_owner(&chunks), Some(owner.clone()));
    let mut future = owner;
    future.version = 2;
    assert!(encode_owner(&future).is_err());
}
