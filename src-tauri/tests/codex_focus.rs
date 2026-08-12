use copet_lib::codex_focus::is_codex_bundle_identifier;

#[test]
fn recognizes_codex_bundle_identifier_only() {
    assert!(is_codex_bundle_identifier(Some("com.openai.codex")));
    assert!(!is_codex_bundle_identifier(Some("com.apple.finder")));
    assert!(!is_codex_bundle_identifier(None));
}
