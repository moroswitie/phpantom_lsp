//! The declaration-hover switch: the command-line route
//! (`Backend::with_hover_on_declarations`, which is what the binary's
//! `--hover-on-declarations` flag feeds) and the
//! `initializationOptions` route (`{"hover": {"declarations": true}}`).
//!
//! Declaration hovers answer nothing, with the switch on or off.
//! These tests drive the public routes that set the switch and assert
//! that a declaration hover is `None`; for the command-line route,
//! the state of the switch itself is asserted inside the crate,
//! where it is visible (the `clone_for_blocking` unit test in
//! `src/backend/construction.rs`).

use crate::common::{create_test_backend, hover_at, initialize_with_options};
use phpantom_lsp::Backend;
use tower_lsp_server::LanguageServer;
use tower_lsp_server::gen_lsp_types::*;

const CONTENT: &str = r#"<?php
class BlogPost {
    public function title(): string {
        return '';
    }
}
"#;

/// Hover on `BlogPost` at its declaration site (line 1, inside the
/// class name).
fn declaration_hover(backend: &Backend) -> Option<Hover> {
    hover_at(backend, "file:///test.php", CONTENT, 1, 8)
}

#[test]
fn with_hover_on_declarations_true_shows_no_declaration_hover() {
    let backend = create_test_backend().with_hover_on_declarations(true);
    assert!(declaration_hover(&backend).is_none());
}

#[test]
fn with_hover_on_declarations_false_shows_no_declaration_hover() {
    let backend = create_test_backend().with_hover_on_declarations(false);
    assert!(declaration_hover(&backend).is_none());
}

#[tokio::test]
async fn initialize_option_top_level_shows_no_declaration_hover() {
    let dir = tempfile::tempdir().unwrap();
    let backend = create_test_backend();
    initialize_with_options(
        &backend,
        dir.path(),
        Some(serde_json::json!({ "hover": { "declarations": true } })),
    )
    .await;
    assert!(declaration_hover(&backend).is_none());
}

#[tokio::test]
async fn initialize_option_under_phpantom_shows_no_declaration_hover() {
    let dir = tempfile::tempdir().unwrap();
    let backend = create_test_backend();
    initialize_with_options(
        &backend,
        dir.path(),
        Some(serde_json::json!({
            "phpantom": { "hover": { "declarations": true } }
        })),
    )
    .await;
    assert!(declaration_hover(&backend).is_none());
}

#[tokio::test]
async fn initialize_option_absent_malformed_or_false_shows_no_declaration_hover() {
    for options in [
        None,
        Some(serde_json::json!({ "unrelated": true })),
        Some(serde_json::json!({ "hover": { "declarations": false } })),
        Some(serde_json::json!({ "hover": { "declarations": "yes" } })),
        Some(serde_json::json!({ "hover": { "declarations": 1 } })),
        Some(serde_json::json!({ "hover": true })),
        Some(serde_json::json!({
            "phpantom": { "hover": { "declarations": false } }
        })),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let backend = create_test_backend();
        initialize_with_options(&backend, dir.path(), options.clone()).await;
        assert!(
            declaration_hover(&backend).is_none(),
            "for initializationOptions {options:?}"
        );
    }
}

#[tokio::test]
async fn method_and_option_combined_shows_no_declaration_hover() {
    let dir = tempfile::tempdir().unwrap();
    let backend = create_test_backend().with_hover_on_declarations(true);
    initialize_with_options(
        &backend,
        dir.path(),
        Some(serde_json::json!({ "hover": { "declarations": true } })),
    )
    .await;
    assert!(declaration_hover(&backend).is_none());
}

#[tokio::test]
async fn method_false_then_option_true_shows_no_declaration_hover() {
    let dir = tempfile::tempdir().unwrap();
    let backend = create_test_backend().with_hover_on_declarations(false);
    initialize_with_options(
        &backend,
        dir.path(),
        Some(serde_json::json!({ "hover": { "declarations": true } })),
    )
    .await;
    assert!(declaration_hover(&backend).is_none());
}

#[tokio::test]
async fn method_true_then_option_false_shows_no_declaration_hover() {
    let dir = tempfile::tempdir().unwrap();
    let backend = create_test_backend().with_hover_on_declarations(true);
    initialize_with_options(
        &backend,
        dir.path(),
        Some(serde_json::json!({ "hover": { "declarations": false } })),
    )
    .await;
    assert!(declaration_hover(&backend).is_none());
}

/// The option is read only during `initialize`: pushing it later
/// through `workspace/didChangeConfiguration` does not turn the
/// feature on, so a declaration hover stays `None`.
#[tokio::test]
async fn did_change_configuration_does_not_read_the_hover_option() {
    let dir = tempfile::tempdir().unwrap();
    let backend = create_test_backend();
    initialize_with_options(&backend, dir.path(), None).await;
    backend
        .did_change_configuration(DidChangeConfigurationParams {
            settings: serde_json::json!({ "hover": { "declarations": true } }),
        })
        .await;
    assert!(declaration_hover(&backend).is_none());
}
