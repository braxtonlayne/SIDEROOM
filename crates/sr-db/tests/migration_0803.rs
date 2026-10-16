//! G-85: the drop of the plaintext signing seed must refuse while a row still holds one, and only
//! then drop the column (the guard runs first, inside the same migration transaction).

#[test]
fn the_drop_is_guarded_by_a_plaintext_check() {
    let sql = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("migrations/0803_drop_plaintext_signing_seed.sql"),
    )
    .unwrap();
    let guard = sql.find("RAISE EXCEPTION").expect("guard");
    let check = sql
        .find("interactions_signing_seed IS NOT NULL")
        .expect("plaintext check");
    let drop = sql
        .find("DROP COLUMN interactions_signing_seed")
        .expect("drop");
    assert!(check < guard && guard < drop, "guard must precede the drop");
    assert!(sql.contains("run the previous release once to seal seeds"));
}
