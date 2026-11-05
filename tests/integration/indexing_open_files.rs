//! The server's state for files that are open in the client: an open
//! buffer, not the file on disk, is what the server knows about that file.

use std::fs;
use std::path::PathBuf;

use crate::common::{
    create_psr4_workspace, definition_locations, goto_definition_at, hover_text_at, open_php,
    position_of,
};
use phpantom_lsp::Backend;
use tower_lsp_server::LanguageServer;
use tower_lsp_server::gen_lsp_types::*;

/// The disk copy declares `Order`; the open buffer has renamed it to
/// `Invoice` (unsaved, one line shorter).  Hovering `Order` in another
/// file makes the class loader follow PSR-4 to the open file's path, and
/// that lookup must not replace the buffer's classes with the disk ones.
#[tokio::test]
async fn lazy_load_of_an_open_file_keeps_the_buffers_classes() {
    let disk = "<?php\nnamespace App;\n\n// saved version\nclass Order {\n    public function __construct(public int $total) {}\n}\n";
    let buffer = "<?php\nnamespace App;\n\nclass Invoice {\n    public function __construct(public int $total) {}\n}\n";
    let user =
        "<?php\nnamespace App;\n\n$old = new Order(1);\n$new = new Invoice(2);\n$new->total;\n";
    let (backend, dir) = create_psr4_workspace(
        r#"{ "autoload": { "psr-4": { "App\\": "src/" } } }"#,
        &[("src/Order.php", disk), ("src/User.php", user)],
    );
    let order_uri = Uri::from_file_path(dir.path().join("src/Order.php")).unwrap();
    let user_uri = Uri::from_file_path(dir.path().join("src/User.php")).unwrap();
    open_php(&backend, &order_uri, buffer).await;
    open_php(&backend, &user_uri, user).await;

    let promoted = position_of(buffer, "$total");
    assert_eq!(
        hover_text_at(&backend, &order_uri, promoted.line, promoted.character + 2).await,
        None,
        "a promoted property's declaration does not hover before the lookup"
    );

    let old = position_of(user, "Order(");
    hover_text_at(&backend, &user_uri, old.line, old.character + 1).await;

    assert_eq!(
        hover_text_at(&backend, &order_uri, promoted.line, promoted.character + 2).await,
        None,
        "the promoted property must not turn into a local-variable hover"
    );

    let invoice = position_of(user, "Invoice(");
    let invoice_hover = hover_text_at(&backend, &user_uri, invoice.line, invoice.character + 1)
        .await
        .expect("the buffer's class must still resolve");
    assert!(
        invoice_hover.contains("class Invoice"),
        "unexpected hover: {invoice_hover}"
    );

    let total = position_of(user, "->total");
    let locations = definition_locations(
        goto_definition_at(&backend, &user_uri, total.line, total.character + 3).await,
    );
    assert_eq!(locations.len(), 1, "definitions: {locations:?}");
    assert_eq!(locations[0].uri, order_uri);
    assert_eq!(
        locations[0].range.start,
        Position {
            line: 4,
            character: promoted.character + 1,
        },
        "the definition must point at the buffer's `$total`"
    );
}

/// A workspace whose Composer `files` autoload names `src/helpers.php`,
/// saved with `old_helper()` and `OLD_LIMIT`, and indexed the way the
/// `initialized` handshake indexes it.
///
/// Also returns the root canonicalized, the spelling the autoload scan
/// indexes the file under, so the URIs a test opens name the file the
/// index does (a temp dir may be spelled otherwise, e.g. with a Windows
/// short name).
async fn autoload_files_workspace() -> (Backend, tempfile::TempDir, PathBuf) {
    let disk = "<?php\n\n// saved version\nfunction old_helper(): int { return 1; }\nconst OLD_LIMIT = 1;\n";
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let root = dunce::canonicalize(dir.path()).unwrap();
    fs::write(root.join("composer.json"), "{}\n").unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/helpers.php"), disk).unwrap();
    fs::create_dir_all(root.join("vendor/composer")).unwrap();
    fs::write(
        root.join("vendor/composer/autoload_files.php"),
        "<?php\n\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir);\n\nreturn array(\n    'abc123' => $baseDir . '/src/helpers.php',\n);\n",
    )
    .unwrap();
    let backend = Backend::new_test_with_workspace(root.clone(), Vec::new());
    backend.initialized(InitializedParams {}).await;
    (backend, dir, root)
}

/// The open buffer of `src/helpers.php` renames both declarations
/// (unsaved, one line shorter), then `user` looks up a name only the disk
/// copy declares, through `lookup`.  That lookup must not replace the
/// buffer's declarations with the disk ones, so `new` still resolves to
/// the buffer's line `line`.
async fn assert_lookup_keeps_the_buffers_declarations(lookup: &str, new: &str, line: u32) {
    let buffer = "<?php\n\nfunction new_helper(): int { return 1; }\nconst NEW_LIMIT = 1;\n";
    let user = "<?php\n\nold_helper();\nnew_helper();\necho OLD_LIMIT;\necho NEW_LIMIT;\n";
    let (backend, _dir, root) = autoload_files_workspace().await;
    let helpers_uri = Uri::from_file_path(root.join("src/helpers.php")).unwrap();
    let user_uri = Uri::from_file_path(root.join("src/user.php")).unwrap();
    open_php(&backend, &helpers_uri, buffer).await;
    open_php(&backend, &user_uri, user).await;

    let old = position_of(user, lookup);
    hover_text_at(&backend, &user_uri, old.line, old.character + 1).await;

    let used = position_of(user, new);
    let locations = definition_locations(
        goto_definition_at(&backend, &user_uri, used.line, used.character + 1).await,
    );
    assert_eq!(locations.len(), 1, "definitions of {new}: {locations:?}");
    assert_eq!(locations[0].uri, helpers_uri);
    assert_eq!(
        locations[0].range.start.line, line,
        "{new} must resolve to the buffer's declaration"
    );
}

/// The function lookup parses the file the autoload index names for
/// `old_helper`, which is the open file.
#[tokio::test]
async fn function_lookup_of_an_open_file_keeps_the_buffers_declarations() {
    assert_lookup_keeps_the_buffers_declarations("old_helper", "new_helper", 2).await;
}

/// The constant lookup parses the file the autoload index names for
/// `OLD_LIMIT`, which is the open file.
#[tokio::test]
async fn constant_lookup_of_an_open_file_keeps_the_buffers_declarations() {
    assert_lookup_keeps_the_buffers_declarations("OLD_LIMIT", "NEW_LIMIT", 3).await;
}

/// The open buffer has moved `Order` from `App` to `Billing` (unsaved).
/// A lookup of `App\Order` follows PSR-4 to the open file, whose buffer
/// declares only `Billing\Order`, and must not take that class for the
/// one it asked for.
#[tokio::test]
async fn lookup_of_a_class_an_open_buffer_moved_namespace_finds_nothing() {
    let disk = "<?php\nnamespace App;\n\nclass Order {}\n";
    let buffer = "<?php\nnamespace Billing;\n\nclass Order {}\n";
    let user = "<?php\nnamespace App;\n\n$order = new Order();\n";
    let (backend, dir) = create_psr4_workspace(
        r#"{ "autoload": { "psr-4": { "App\\": "src/" } } }"#,
        &[("src/Order.php", disk), ("src/User.php", user)],
    );
    let order_uri = Uri::from_file_path(dir.path().join("src/Order.php")).unwrap();
    let user_uri = Uri::from_file_path(dir.path().join("src/User.php")).unwrap();
    open_php(&backend, &order_uri, buffer).await;
    open_php(&backend, &user_uri, user).await;

    let order = position_of(user, "Order(");
    let hover = hover_text_at(&backend, &user_uri, order.line, order.character + 1).await;
    assert!(
        hover
            .as_deref()
            .is_none_or(|text| !text.contains("Billing")),
        "App\\Order must not resolve to Billing\\Order: {hover:?}"
    );
}

/// A file that declares `Order` in two namespace blocks: a lookup of
/// `App\Order` must pick the `App` block's class, not the first `Order`.
#[tokio::test]
async fn lookup_in_a_file_with_several_namespace_blocks_picks_the_named_class() {
    let order = "<?php\nnamespace Billing {\n    class Order { public function billing(): void {} }\n}\n\nnamespace App {\n    class Order { public function app(): void {} }\n}\n";
    let user = "<?php\nnamespace App;\n\n$order = new Order();\n$order->app();\n";
    let (backend, dir) = create_psr4_workspace(
        r#"{ "autoload": { "psr-4": { "App\\": "src/" } } }"#,
        &[("src/Order.php", order), ("src/User.php", user)],
    );
    let order_uri = Uri::from_file_path(dir.path().join("src/Order.php")).unwrap();
    let user_uri = Uri::from_file_path(dir.path().join("src/User.php")).unwrap();
    open_php(&backend, &user_uri, user).await;

    let app = position_of(user, "->app");
    let locations = definition_locations(
        goto_definition_at(&backend, &user_uri, app.line, app.character + 3).await,
    );
    assert_eq!(locations.len(), 1, "definitions of app: {locations:?}");
    assert_eq!(locations[0].uri, order_uri);
    assert_eq!(locations[0].range.start.line, 6);
}
