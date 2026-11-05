//! Declaration hover for local `static` and `global` declarations.
//!
//! A `static $name` declaration answers the ordinary variable hover when
//! the switch is on. A `global $name` import answers it in either mode:
//! the type the shared resolver gives the variable there, or the plain
//! `$name` when it gives none. Closure
//! `use (...)` captures, docblock `@param` / `@var` names and `unset`
//! keep answering `None`, and every other variable definition keeps
//! the hover it had.

use crate::common::{
    create_test_backend, definition_locations, goto_definition_at, hover_at, hover_response,
    hover_text, open_php, position_of,
};
use phpantom_lsp::Backend;
use tower_lsp_server::gen_lsp_types::*;

fn on_backend() -> Backend {
    create_test_backend().with_hover_on_declarations(true)
}

fn hover_text_at(
    backend: &Backend,
    content: &str,
    (line, character): (u32, u32),
) -> Option<String> {
    hover_at(backend, "file:///test.php", content, line, character)
        .map(|hover| hover_text(&hover).to_string())
}

/// The range of `token` inside the first occurrence of `search`,
/// which must sit on one line.
fn range_of_token(content: &str, search: &str, token: &str) -> Range {
    let start = position_of(content, search);
    let token_start = search
        .find(token)
        .unwrap_or_else(|| panic!("{token:?} is not inside {search:?}"));
    let start = Position {
        line: start.line,
        character: start.character + search[..token_start].encode_utf16().count() as u32,
    };
    Range {
        start,
        end: Position {
            line: start.line,
            character: start.character + token.encode_utf16().count() as u32,
        },
    }
}

/// The (line, character) just inside `range`.
fn inside(range: Range) -> (u32, u32) {
    (range.start.line, range.start.character + 1)
}

/// Assert the off/on pair for `token` inside the first occurrence of
/// `search`: off answers nothing, on answers a hover over exactly that
/// token whose text is `expected`.
fn assert_off_on(content: &str, search: &str, token: &str, expected: &str) {
    let range = range_of_token(content, search, token);
    let off = hover_text_at(&create_test_backend(), content, inside(range));
    assert!(
        off.is_none(),
        "off should answer nothing on {token}, got: {off:?}"
    );
    let (line, character) = inside(range);
    let hover = hover_at(&on_backend(), "file:///test.php", content, line, character)
        .unwrap_or_else(|| panic!("on should hover on {token}"));
    assert_eq!(hover_text(&hover), expected, "hover on {token}");
    assert_eq!(
        hover.range,
        Some(range),
        "hover on {token} should cover exactly that token"
    );
}

fn assert_hover_both_modes(content: &str, search: &str, token: &str, expected: &str) {
    let range = range_of_token(content, search, token);
    let (line, character) = inside(range);
    for enabled in [false, true] {
        let backend = create_test_backend().with_hover_on_declarations(enabled);
        let hover = hover_at(&backend, "file:///test.php", content, line, character)
            .unwrap_or_else(|| {
                panic!("global import {token} should hover with declarations {enabled}")
            });
        assert_eq!(hover_text(&hover), expected, "hover on {token}");
        assert_eq!(
            hover.range,
            Some(range),
            "hover on {token} should cover exactly that token"
        );
    }
}

/// The markdown of a variable hover showing `code`.
fn variable_hover(code: &str) -> String {
    format!("```php\n<?php\n{code}\n```")
}

// ─── static ─────────────────────────────────────────────────────────

const STATICS: &str = r#"<?php
class Logger {}

function tick(): void
{
    static $count = 0;
    static $never;
    static $first = 'a', $second = 2;
    static $logger;
    $logger = new Logger();
    $seen = $count;
}
"#;

#[test]
fn static_declaration_shows_its_initialisers_type() {
    assert_off_on(
        STATICS,
        "static $count = 0",
        "$count",
        &variable_hover("$count = 0"),
    );
}

#[test]
fn static_declaration_without_a_type_shows_the_plain_variable() {
    assert_off_on(
        STATICS,
        "static $never",
        "$never",
        &variable_hover("$never"),
    );
}

#[test]
fn each_name_of_a_multi_variable_static_declaration_shows_its_own_hover() {
    let search = "static $first = 'a', $second = 2";
    assert_off_on(STATICS, search, "$first", &variable_hover("$first = 'a'"));
    assert_off_on(STATICS, search, "$second", &variable_hover("$second = 2"));
}

/// A `static` keeps what an earlier call stored, so its declaration
/// shows the types assigned anywhere in the body, as a read of it does.
#[test]
fn static_declaration_shows_a_type_assigned_later_in_the_body() {
    assert_off_on(
        STATICS,
        "static $logger",
        "$logger",
        &variable_hover("$logger = Logger"),
    );
}

#[test]
fn static_declaration_hover_matches_a_later_read() {
    let backend = on_backend();
    let declared = hover_text_at(
        &backend,
        STATICS,
        inside(range_of_token(STATICS, "static $count", "$count")),
    )
    .expect("the declaration should hover");
    let read = hover_text_at(
        &backend,
        STATICS,
        inside(range_of_token(STATICS, "$seen = $count", "$count")),
    )
    .expect("the read should hover");
    assert_eq!(declared, read);
}

// ─── global ─────────────────────────────────────────────────────────

const GLOBALS: &str = r#"<?php
class Database {}
class Logger {}

$db = new Database();

function work(): void
{
    global $db, $missing;
    /** @var Logger $log */
    global $log;
    $used = $db;
}
"#;

#[test]
fn global_declaration_shows_the_top_level_type() {
    assert_hover_both_modes(
        GLOBALS,
        "global $db, $missing",
        "$db",
        &variable_hover("$db = Database"),
    );
}

#[test]
fn global_declaration_of_an_unassigned_variable_shows_the_plain_variable() {
    assert_hover_both_modes(
        GLOBALS,
        "global $db, $missing",
        "$missing",
        &variable_hover("$missing"),
    );
}

#[test]
fn global_declaration_shows_its_var_docblock_type() {
    assert_hover_both_modes(
        GLOBALS,
        "global $log",
        "$log",
        &variable_hover("$log = Logger"),
    );
}

#[test]
fn global_declaration_hover_matches_a_later_read() {
    let backend = on_backend();
    let declared = hover_text_at(
        &backend,
        GLOBALS,
        inside(range_of_token(GLOBALS, "global $db", "$db")),
    )
    .expect("the declaration should hover");
    let read = hover_text_at(
        &backend,
        GLOBALS,
        inside(range_of_token(GLOBALS, "$used = $db", "$db")),
    )
    .expect("the read should hover");
    assert_eq!(declared, read);
}

// ─── Location round trip ────────────────────────────────────────────

#[tokio::test]
async fn definition_positions_of_reads_hover_as_declarations() {
    let backend = on_backend();
    for (file, content, read, token, declaration, expected) in [
        (
            "file:///statics.php",
            STATICS,
            "$seen = $count",
            "$count",
            "static $count",
            "$count = 0",
        ),
        (
            "file:///globals.php",
            GLOBALS,
            "$used = $db",
            "$db",
            "global $db",
            "$db = Database",
        ),
    ] {
        let uri = Uri::parse(file).unwrap();
        open_php(&backend, &uri, content).await;
        let (line, character) = inside(range_of_token(content, read, token));
        let locations =
            definition_locations(goto_definition_at(&backend, &uri, line, character).await);
        let location = locations
            .first()
            .unwrap_or_else(|| panic!("definition of {token} should exist"));
        assert_eq!(
            location.range.start,
            range_of_token(content, declaration, token).start,
            "definition of {token} should land on its declaration"
        );
        let hover = hover_response(
            &backend,
            &uri,
            location.range.start.line,
            location.range.start.character,
        )
        .await
        .unwrap_or_else(|| panic!("definition position of {token} should hover"));
        assert_eq!(hover_text(&hover), variable_hover(expected), "{token}");
    }
}

// ─── Unchanged kinds ────────────────────────────────────────────────

#[test]
fn captures_docblock_names_and_unset_answer_nothing_with_the_switch_on() {
    let content = r#"<?php
/**
 * @param int $documented
 */
function keep(int $documented, int $captured, int $gone): void
{
    /** @var int $annotated */
    $annotated = 1;
    $closure = function () use ($captured) {};
    unset($gone);
}
"#;
    for (search, token) in [
        ("@param int $documented", "$documented"),
        ("@var int $annotated", "$annotated"),
        ("use ($captured)", "$captured"),
        ("unset($gone)", "$gone"),
    ] {
        let position = inside(range_of_token(content, search, token));
        for backend in [create_test_backend(), on_backend()] {
            let hover = hover_text_at(&backend, content, position);
            assert!(
                hover.is_none(),
                "{token} in {search:?} should answer nothing, got: {hover:?}"
            );
        }
    }
}

#[test]
fn other_variable_definitions_keep_their_hover_with_the_switch_on() {
    let content = r#"<?php
class Item {}

/**
 * @param list<Item> $items
 */
function run(Item $param, array $items): void
{
    $assigned = new Item();
    foreach ($items as $entry) {}
    try {} catch (\Exception $caught) {}
}
"#;
    for (search, token, expected) in [
        ("Item $param", "$param", "$param = Item"),
        ("$assigned = new", "$assigned", "$assigned = Item"),
        ("as $entry", "$entry", "$entry = Item"),
        ("\\Exception $caught", "$caught", "$caught = Exception"),
    ] {
        let position = inside(range_of_token(content, search, token));
        for backend in [create_test_backend(), on_backend()] {
            let hover = hover_text_at(&backend, content, position);
            assert_eq!(
                hover.as_deref(),
                Some(variable_hover(expected).as_str()),
                "{token}"
            );
        }
    }
}
