//! Global constants declared more than once or in unusual spellings
//! (D-12 to D-14, issues 5 to 8): every declaration a run could take is
//! shown and navigable, and every spelling PHP accepts is indexed.

use crate::common::{
    create_psr4_workspace, create_test_backend, goto_definition_at, hover_response, hover_text,
    offset_position, open_php,
};
use phpantom_lsp::Backend;
use tower_lsp_server::gen_lsp_types::*;

/// The position `delta` characters into the `nth` (0-based) occurrence
/// of `needle` in `content`.
fn position_at(content: &str, needle: &str, delta: u32, nth: usize) -> Position {
    let (offset, _) = content
        .match_indices(needle)
        .nth(nth)
        .unwrap_or_else(|| panic!("occurrence {nth} of {needle:?} not found"));
    offset_position(content, offset + delta as usize)
}

/// The hover markdown at `needle` in `content`, already open as `uri`.
async fn hover_markdown(
    backend: &Backend,
    uri: &Uri,
    content: &str,
    needle: &str,
    delta: u32,
) -> Option<String> {
    let position = position_at(content, needle, delta, 0);
    hover_response(backend, uri, position.line, position.character)
        .await
        .map(|hover| hover_text(&hover).to_string())
}

/// The definitions at `needle` in `content`, already open as `uri`, as
/// `(file name, 1-based line)` in response order.
async fn definitions(
    backend: &Backend,
    uri: &Uri,
    content: &str,
    needle: &str,
    delta: u32,
) -> Vec<(String, u32)> {
    let position = position_at(content, needle, delta, 0);
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

/// A backend with `content` open as `file:///main.php`.
async fn open_main(content: &str) -> (Backend, Uri) {
    let backend = create_test_backend();
    let uri = Uri::parse("file:///main.php").unwrap();
    open_php(&backend, &uri, content).await;
    (backend, uri)
}

fn expect(pairs: &[(&str, u32)]) -> Vec<(String, u32)> {
    pairs
        .iter()
        .map(|(name, line)| (name.to_string(), *line))
        .collect()
}

const C1_VERSION_GUARD: &str = r#"<?php
if (PHP_VERSION_ID >= 80000) {
    define('MODE', 'new');
} else {
    define('MODE', 'old');
}
echo MODE;
"#;

const C3_DEFINE_LEADING_BACKSLASH: &str = r#"<?php
define('\\LIMIT', 1);
echo LIMIT;
"#;

const C4_DEFINED_OR_DEFINE: &str = r#"<?php
defined('ORX') or define('ORX', 5);
echo ORX;
"#;

const C5_FQ_DEFINE: &str = r#"<?php
namespace App;
\define('GLOBAL_NAME', 20);
echo \GLOBAL_NAME;
"#;

/// `define('\LIMIT', …)` declares a constant no unqualified `LIMIT` ever
/// reaches, so nothing hovers.
#[tokio::test]
async fn d13_a_leading_backslash_define_never_resolves() {
    let (backend, uri) = open_main(C3_DEFINE_LEADING_BACKSLASH).await;
    let hover = hover_markdown(&backend, &uri, C3_DEFINE_LEADING_BACKSLASH, "echo LIMIT", 6).await;
    assert_eq!(hover, None);
}

/// A constant defined in both branches of a version guard hovers with
/// both values, in source order.
#[tokio::test]
#[ignore = "issue 7 / D-12: only the last define() of a version guard is kept"]
async fn issue_7_version_guard_hover_lists_both_values() {
    let (backend, uri) = open_main(C1_VERSION_GUARD).await;
    let markdown = hover_markdown(&backend, &uri, C1_VERSION_GUARD, "echo MODE", 6)
        .await
        .expect("expected a hover on MODE");
    let new = markdown.find("MODE = 'new'");
    let old = markdown.find("MODE = 'old'");
    assert!(
        new.is_some() && old.is_some() && new < old,
        "both values, 'new' first:\n{markdown}"
    );
}

/// Go-to-definition on a version-guarded constant offers both `define()`s.
#[tokio::test]
#[ignore = "issue 7 / D-12: only the last define() of a version guard is kept"]
async fn issue_7_version_guard_definition_returns_both() {
    let (backend, uri) = open_main(C1_VERSION_GUARD).await;
    let found = definitions(&backend, &uri, C1_VERSION_GUARD, "echo MODE", 6).await;
    assert_eq!(found, expect(&[("main.php", 3), ("main.php", 5)]));
}

/// A value read from a version-guarded constant has the union of both
/// values' types.
#[tokio::test]
#[ignore = "issue 7 / D-12: only the last define() of a version guard is kept"]
async fn issue_7_version_guard_type_is_the_union() {
    let content = format!("{C1_VERSION_GUARD}$v = MODE;\n");
    let (backend, uri) = open_main(&content).await;
    let markdown = hover_markdown(&backend, &uri, &content, "$v =", 1)
        .await
        .expect("expected a hover on $v");
    assert!(markdown.contains("$v = 'new'|'old'\n"), "{markdown}");
}

/// A `const` declared in two files hovers with both values and navigates
/// to both, whichever file was opened first.
#[tokio::test]
#[ignore = "issue 7 / D-12: of two files declaring a const, only one is kept"]
async fn issue_7_cross_file_const_lists_both_in_any_open_order() {
    let a = "<?php\nconst SHARED = 'from-a';\n";
    let b = "<?php\nconst SHARED = 'from-b';\n";
    let main = "<?php\nrequire 'a.php';\necho SHARED;\n";
    for order in [["a.php", "b.php"], ["b.php", "a.php"]] {
        let (backend, dir) =
            create_psr4_workspace("{}", &[("a.php", a), ("b.php", b), ("main.php", main)]);
        for name in order {
            let uri = Uri::from_file_path(dir.path().join(name)).unwrap();
            open_php(&backend, &uri, if name == "a.php" { a } else { b }).await;
        }
        let uri = Uri::from_file_path(dir.path().join("main.php")).unwrap();
        open_php(&backend, &uri, main).await;

        let markdown = hover_markdown(&backend, &uri, main, "echo SHARED", 6)
            .await
            .unwrap_or_else(|| panic!("{order:?}: expected a hover on SHARED"));
        let from_a = markdown.find("'from-a'");
        let from_b = markdown.find("'from-b'");
        assert!(
            from_a.is_some() && from_b.is_some() && from_a < from_b,
            "{order:?}: both values, 'from-a' first:\n{markdown}"
        );
        let found = definitions(&backend, &uri, main, "echo SHARED", 6).await;
        assert_eq!(found, expect(&[("a.php", 2), ("b.php", 2)]), "{order:?}");
    }
}

/// `defined('X') or define('X', …)` declares `X`.
#[tokio::test]
async fn issue_6_defined_or_define_is_indexed() {
    let (backend, uri) = open_main(C4_DEFINED_OR_DEFINE).await;
    let markdown = hover_markdown(&backend, &uri, C4_DEFINED_OR_DEFINE, "echo ORX", 6)
        .await
        .expect("expected a hover on ORX");
    assert!(markdown.contains("ORX = 5"), "{markdown}");
}

/// A fully qualified `\define()` call inside a namespace declares a global
/// constant.
#[tokio::test]
async fn issue_8_fully_qualified_define_is_indexed() {
    let (backend, uri) = open_main(C5_FQ_DEFINE).await;
    let markdown = hover_markdown(&backend, &uri, C5_FQ_DEFINE, "echo \\GLOBAL_NAME", 7)
        .await
        .expect("expected a hover on \\GLOBAL_NAME");
    assert!(markdown.contains("GLOBAL_NAME = 20"), "{markdown}");
}
