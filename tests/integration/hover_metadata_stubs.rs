//! Which declaration a `[stubs]` path overrides (D-15, issue 9): a
//! project declaration wins over a stub of the same name, and a stub
//! still wins over vendor code.

use crate::common::{goto_definition_at, hover_response, hover_text, offset_position, open_php};
use phpantom_lsp::Backend;
use std::fs;
use std::path::Path;
use tower_lsp_server::LanguageServer;
use tower_lsp_server::gen_lsp_types::*;

const PROJECT: &str = "<?php\n/** Live. */\nclass Overridden { public function answer(): int { return 1; } }\nfunction customHelper(): int { return 1; }\nfunction run() { $h = customHelper(); $o = new Overridden(); $a = $o->answer(); }\n";

const PROJECT_WITH_VENDOR_CALL: &str = "<?php\n/** Live. */\nclass Overridden { public function answer(): int { return 1; } }\nfunction customHelper(): int { return 1; }\nfunction run() { $h = customHelper(); $o = new Overridden(); $a = $o->answer(); $r = (new \\Acme\\Lib\\Client())->post(); }\n";

const STUB: &str = "<?php\n/** Stub. */\nclass Overridden { public function answer(): string {} }\nfunction customHelper(): string {}\n";

fn write(root: &Path, rel: &str, content: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// The C6 layout (`.phpantom.toml` with `[stubs] paths = ["z-stubs"]`,
/// `a-project.php`, `z-stubs/override.php`), with the server initialised
/// and `project` open as `a-project.php`.
async fn stub_workspace(
    project: &str,
    extra: &[(&str, &str)],
) -> (Backend, tempfile::TempDir, Uri) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, ".phpantom.toml", "[stubs]\npaths = [\"z-stubs\"]\n");
    write(root, "a-project.php", project);
    write(root, "z-stubs/override.php", STUB);
    for (rel, content) in extra {
        write(root, rel, content);
    }

    let backend = Backend::new_test_with_workspace(root.to_path_buf(), Vec::new());
    backend
        .initialize(InitializeParams::default())
        .await
        .unwrap();
    backend.initialized(InitializedParams {}).await;
    let uri = Uri::from_file_path(root.join("a-project.php")).unwrap();
    open_php(&backend, &uri, project).await;
    (backend, dir, uri)
}

/// The position `delta` characters into the first occurrence of `needle`.
fn position_at(content: &str, needle: &str, delta: u32) -> Position {
    let offset = content
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} not found"));
    offset_position(content, offset + delta as usize)
}

async fn hover_markdown(
    backend: &Backend,
    uri: &Uri,
    content: &str,
    needle: &str,
    delta: u32,
) -> String {
    let position = position_at(content, needle, delta);
    let hover = hover_response(backend, uri, position.line, position.character)
        .await
        .unwrap_or_else(|| panic!("expected a hover at {needle:?}"));
    hover_text(&hover).to_string()
}

/// The definitions at `needle`, as `(file name, 1-based line)` in response
/// order.
async fn definitions(
    backend: &Backend,
    uri: &Uri,
    content: &str,
    needle: &str,
    delta: u32,
) -> Vec<(String, u32)> {
    let position = position_at(content, needle, delta);
    let file_line = |uri: &Uri, line: u32| {
        let name = uri.as_str().rsplit('/').next().unwrap_or_default();
        (name.to_string(), line + 1)
    };
    match goto_definition_at(backend, uri, position.line, position.character).await {
        None => Vec::new(),
        Some(DefinitionResponse::Definition(Definition::Location(location))) => {
            vec![file_line(&location.uri, location.range.start.line)]
        }
        Some(DefinitionResponse::Definition(Definition::LocationList(locations))) => locations
            .iter()
            .map(|location| file_line(&location.uri, location.range.start.line))
            .collect(),
        Some(DefinitionResponse::DefinitionLinkList(links)) => links
            .iter()
            .map(|link| file_line(&link.target_uri, link.target_selection_range.start.line))
            .collect(),
    }
}

async fn assert_var_type(backend: &Backend, uri: &Uri, content: &str, var: &str, expected: &str) {
    let markdown = hover_markdown(backend, uri, content, &format!("{var} ="), 1).await;
    assert!(
        markdown.contains(&format!("{var} = {expected}\n")),
        "{var}: expected `{expected}`, got:\n{markdown}"
    );
}

/// Navigation to a project function and class, the class's hover and the
/// type of its method all take the project declaration over the stub.
#[tokio::test]
async fn d15_project_declarations_win_over_a_stub_controls() {
    let (backend, _dir, uri) = stub_workspace(PROJECT, &[]).await;

    assert_eq!(
        definitions(&backend, &uri, PROJECT, "= customHelper", 3).await,
        vec![("a-project.php".to_string(), 4)]
    );
    let markdown = hover_markdown(&backend, &uri, PROJECT, "new Overridden", 5).await;
    assert!(markdown.contains("Live."), "project docblock: {markdown}");
    assert!(!markdown.contains("Stub."), "no stub docblock: {markdown}");
    assert_eq!(
        definitions(&backend, &uri, PROJECT, "new Overridden", 5).await,
        vec![("a-project.php".to_string(), 3)]
    );
    assert_var_type(&backend, &uri, PROJECT, "$a", "int").await;
}

/// A stub of a vendor class still wins over the vendor's own declaration.
#[tokio::test]
async fn d15_a_stub_still_wins_over_vendor_code() {
    let (backend, _dir, uri) = stub_workspace(
        PROJECT_WITH_VENDOR_CALL,
        &[
            ("composer.json", "{}"),
            (
                "vendor/composer/installed.json",
                r#"{ "packages": [ {
            "name": "acme/lib",
            "install-path": "../acme/lib",
            "autoload": { "psr-4": { "Acme\\Lib\\": "src/" } }
        } ] }"#,
            ),
            (
                "vendor/acme/lib/src/Client.php",
                "<?php\nnamespace Acme\\Lib;\nclass Client { public function post(): int { return 1; } }\n",
            ),
            (
                "z-stubs/acme.php",
                "<?php\nnamespace Acme\\Lib;\nclass Client { public function post(): string {} }\n",
            ),
        ],
    )
    .await;

    assert_var_type(&backend, &uri, PROJECT_WITH_VENDOR_CALL, "$r", "string").await;
}

/// A project function wins over a stub function of the same name, as a
/// project class does.
#[tokio::test]
#[ignore = "issue 9 / D-15: the type engine takes the stub function over the project function"]
async fn issue_9_a_project_function_wins_over_a_stub() {
    let (backend, _dir, uri) = stub_workspace(PROJECT, &[]).await;
    assert_var_type(&backend, &uri, PROJECT, "$h", "int").await;
}
