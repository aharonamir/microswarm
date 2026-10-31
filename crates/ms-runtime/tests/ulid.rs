#[test]
fn new_ids_are_canonical_ulids() {
    let alphabet = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut unique = std::collections::HashSet::new();
    for _ in 0..100 {
        let id = ms_runtime::new_id().unwrap();
        assert_eq!(id.len(), 26);
        assert!(id.chars().all(|character| alphabet.contains(character)));
        assert!(unique.insert(id));
    }
}
