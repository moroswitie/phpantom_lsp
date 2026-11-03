//! Declaration hover: the switch (the command-line route
//! `Backend::with_hover_on_declarations`, which is what the binary's
//! `--hover-on-declarations` flag feeds, and the
//! `initializationOptions` route `{"hover": {"declarations": true}}`)
//! plus the class-like, method, class-constant, enum-case, function
//! and top-level constant declarations it turns on.
//!
//! With the switch off every declaration answers `None`, exactly as
//! before. With it on, a declaration answers the same hover a usage
//! of the symbol answers, for that very declaration: where PHPantom
//! holds another declaration of the same name instead, it answers
//! `None`, never the other one's hover. Properties, promoted
//! parameters and docblock `@property` / `@method` tags are covered in
//! `hover_declarations_members.rs`.

use std::sync::Arc;

use crate::common::{
    create_test_backend, definition_locations, goto_definition_at, hover_at, hover_response,
    hover_text, initialize_with_options, open_php, open_php_str, position_of, position_of_last,
};
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

fn on_backend() -> Backend {
    create_test_backend().with_hover_on_declarations(true)
}

/// Hover on `BlogPost` at its declaration site (line 1, inside the
/// class name).
fn declaration_hover(backend: &Backend) -> Option<Hover> {
    hover_at(backend, "file:///test.php", CONTENT, 1, 8)
}

fn hover_on_text(backend: &Backend, content: &str, line: u32, character: u32) -> Option<String> {
    hover_at(backend, "file:///test.php", content, line, character)
        .map(|hover| hover_text(&hover).to_string())
}

/// The (line, character) just inside the first occurrence of `needle`.
fn inside(content: &str, needle: &str) -> (u32, u32) {
    let position = position_of(content, needle);
    (position.line, position.character + 1)
}

/// The (line, character) just inside the last occurrence of `needle`.
fn inside_last(content: &str, needle: &str) -> (u32, u32) {
    let position = position_of_last(content, needle);
    (position.line, position.character + 1)
}

fn hover_inside(backend: &Backend, content: &str, needle: &str) -> Option<Hover> {
    let (line, character) = inside(content, needle);
    hover_at(backend, "file:///test.php", content, line, character)
}

fn hover_inside_last(backend: &Backend, content: &str, needle: &str) -> Option<Hover> {
    let (line, character) = inside_last(content, needle);
    hover_at(backend, "file:///test.php", content, line, character)
}

/// The (line, character) just inside `token`, where `token` occurs
/// inside the first occurrence of `search` in `content`. This lets a
/// test locate `function add` but put the cursor on `add` itself.
fn inside_token(content: &str, search: &str, token: &str) -> (u32, u32) {
    let start = range_of_token(content, search, token).start;
    (start.line, start.character + 1)
}

fn hover_inside_token(
    backend: &Backend,
    content: &str,
    search: &str,
    token: &str,
) -> Option<Hover> {
    let (line, character) = inside_token(content, search, token);
    hover_at(backend, "file:///test.php", content, line, character)
}

/// Assert the off/on pair for the token `needle` (located by the
/// first occurrence of `needle` itself): off answers nothing, on
/// answers a hover over exactly that token containing every fragment.
/// Returns the on-hover for further assertions.
fn assert_off_on(content: &str, needle: &str, fragments: &[&str]) -> Hover {
    assert_off_on_token(content, needle, needle, fragments)
}

/// [`assert_off_on`] with the cursor on `token` inside `search`.
fn assert_off_on_token(content: &str, search: &str, token: &str, fragments: &[&str]) -> Hover {
    let (line, character) = inside_token(content, search, token);
    let off = hover_on_text(&create_test_backend(), content, line, character);
    assert!(off.is_none(), "off should answer nothing, got: {off:?}");
    let hover = hover_at(&on_backend(), "file:///test.php", content, line, character)
        .unwrap_or_else(|| panic!("on should hover on {token}"));
    let text = hover_text(&hover);
    for fragment in fragments {
        assert!(
            text.contains(fragment),
            "hover on {token} should contain {fragment:?}, got: {text}"
        );
    }
    assert_eq!(
        hover.range,
        Some(range_of_token(content, search, token)),
        "hover on {token} should cover exactly that token"
    );
    hover
}

/// The range of `token` inside the first occurrence of `search`,
/// which must sit on one line.
fn range_of_token(content: &str, search: &str, token: &str) -> Range {
    let start = position_of(content, search);
    let token_start = search
        .find(token)
        .unwrap_or_else(|| panic!("{token:?} is not inside {search:?}"));
    token_range(
        Position {
            line: start.line,
            character: start.character + search[..token_start].encode_utf16().count() as u32,
        },
        token,
    )
}

/// The range of `token` written at `start`.
fn token_range(start: Position, token: &str) -> Range {
    Range {
        start,
        end: Position {
            line: start.line,
            character: start.character + token.encode_utf16().count() as u32,
        },
    }
}

// ─── The switch ─────────────────────────────────────────────────────

#[test]
fn with_hover_on_declarations_true_shows_declaration_hover() {
    let backend = on_backend();
    let hover = declaration_hover(&backend).expect("on should show the class hover");
    assert!(hover_text(&hover).contains("class BlogPost"));
}

#[test]
fn with_hover_on_declarations_false_shows_no_declaration_hover() {
    let backend = create_test_backend().with_hover_on_declarations(false);
    assert!(declaration_hover(&backend).is_none());
}

#[tokio::test]
async fn initialize_option_top_level_shows_declaration_hover() {
    let dir = tempfile::tempdir().unwrap();
    let backend = create_test_backend();
    initialize_with_options(
        &backend,
        dir.path(),
        Some(serde_json::json!({ "hover": { "declarations": true } })),
    )
    .await;
    assert!(declaration_hover(&backend).is_some());
}

#[tokio::test]
async fn initialize_option_under_phpantom_shows_declaration_hover() {
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
    assert!(declaration_hover(&backend).is_some());
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
async fn method_and_option_combined_shows_declaration_hover() {
    let dir = tempfile::tempdir().unwrap();
    let backend = on_backend();
    initialize_with_options(
        &backend,
        dir.path(),
        Some(serde_json::json!({ "hover": { "declarations": true } })),
    )
    .await;
    assert!(declaration_hover(&backend).is_some());
}

#[tokio::test]
async fn method_false_then_option_true_shows_declaration_hover() {
    let dir = tempfile::tempdir().unwrap();
    let backend = create_test_backend().with_hover_on_declarations(false);
    initialize_with_options(
        &backend,
        dir.path(),
        Some(serde_json::json!({ "hover": { "declarations": true } })),
    )
    .await;
    assert!(declaration_hover(&backend).is_some());
}

#[tokio::test]
async fn method_true_then_option_false_still_shows_declaration_hover() {
    let dir = tempfile::tempdir().unwrap();
    let backend = on_backend();
    initialize_with_options(
        &backend,
        dir.path(),
        Some(serde_json::json!({ "hover": { "declarations": false } })),
    )
    .await;
    assert!(declaration_hover(&backend).is_some());
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

// ─── Class-like declarations ────────────────────────────────────────

#[test]
fn class_name_declaration_shows_the_class_hover() {
    let content = r#"<?php
/** Represents a blog post. */
class BlogPost {
    public string $title;
}
$post = new BlogPost();
"#;
    let hover = assert_off_on_token(content, "class BlogPost", "BlogPost", &["class BlogPost"]);
    let text = hover_text(&hover);
    assert!(
        text.contains("Represents a blog post."),
        "should show the class description: {text}"
    );

    // Identical to hovering a reference to the same class.
    let usage = hover_inside_token(&on_backend(), content, "new BlogPost", "BlogPost")
        .expect("usage should hover");
    // The `new` usage follows the constructor rule when one exists;
    // this class has none, so both read as the class hover.
    assert_eq!(hover_text(&usage), text);
}

#[test]
fn class_keyword_declaration_shows_the_class_hover() {
    let content = r#"<?php
/** Represents a blog post. */
class BlogPost {
    public string $title;
}
"#;
    let name = assert_off_on_token(content, "class BlogPost", "BlogPost", &["class BlogPost"]);
    let keyword = assert_off_on_token(content, "class BlogPost", "class", &["class BlogPost"]);
    assert_eq!(hover_text(&keyword), hover_text(&name));
}

#[test]
fn interface_trait_and_enum_keywords_show_their_hover() {
    let content = r#"<?php
interface Renderable {}
trait Timestamps {}
enum Suit {}
echo 'café'; class AfterUnicode {}
"#;
    for (search, keyword) in [
        ("interface Renderable", "interface"),
        ("trait Timestamps", "trait"),
        ("enum Suit", "enum"),
        // The keyword range after non-ASCII text on the same line.
        ("class AfterUnicode", "class"),
    ] {
        assert_off_on_token(content, search, keyword, &[search]);
    }
}

#[test]
fn class_keyword_hover_skips_modifiers_and_the_rest_of_the_header() {
    let content = r#"<?php
final class FinalPost {}
class Base {}
abstract class AbstractPost extends Base {}
"#;
    assert_off_on_token(content, "final class", "class", &["final class FinalPost"]);
    assert_off_on_token(
        content,
        "abstract class",
        "class",
        &["abstract class AbstractPost"],
    );
    let backend = on_backend();
    for needle in ["final class", "abstract class", "extends"] {
        assert!(
            hover_inside(&backend, content, needle).is_none(),
            "{needle:?} is not a declaration hover position"
        );
    }
}

#[test]
fn class_keyword_hover_reaches_one_past_the_keyword() {
    let content = "<?php\nclass BlogPost {}\n";
    let keyword = range_of_token(content, "class BlogPost", "class");
    let hover = hover_at(
        &on_backend(),
        "file:///test.php",
        content,
        keyword.end.line,
        keyword.end.character,
    )
    .expect("the end of the keyword still reads as on it");
    assert_eq!(hover.range, Some(keyword));
    assert!(hover_text(&hover).contains("class BlogPost"));
}

#[test]
fn a_second_declaration_of_a_class_never_shows_the_first_ones_hover() {
    // One declaration per branch of a version guard. PHPantom keeps
    // only the first, so the second has no hover of its own.
    let content = r#"<?php
if (PHP_VERSION_ID >= 80000) {
    /** First variant. */
    class Poly {
        public function first(): int { return 1; }
    }
} else {
    /** Second variant. */
    class Poly {
        public function second(): string { return ''; }
    }
}
"#;
    assert_off_on_token(content, "class Poly", "Poly", &["First variant."]);
    assert_off_on_token(content, "class Poly", "class", &["First variant."]);
    assert_off_on_token(
        content,
        "function first",
        "first",
        &["function first(): int"],
    );

    let backend = on_backend();
    let second = position_of_last(content, "class Poly");
    for (character, what) in [
        (second.character + 1, "keyword"),
        (second.character + 7, "name"),
    ] {
        let hover = hover_at(
            &backend,
            "file:///test.php",
            content,
            second.line,
            character,
        );
        assert!(
            hover.is_none(),
            "the second declaration's {what} should answer nothing, got: {hover:?}"
        );
    }
    assert!(hover_inside_last(&backend, content, "second()").is_none());
}

#[test]
fn interface_trait_and_enum_declarations_show_their_hover() {
    let content = r#"<?php
interface Renderable {
    public function render(): string;
}
trait Timestamps {
    public function touch(): void {}
}
enum Suit {
    case Hearts;
    case Spades;
}
enum Colour: string {
    case Red = 'red';
}
"#;
    assert_off_on(content, "Renderable", &["interface Renderable"]);
    assert_off_on(content, "Timestamps", &["trait Timestamps"]);
    let suit = assert_off_on(content, "Suit", &["enum Suit", "case Hearts"]);
    assert!(hover_text(&suit).contains("case Spades"));
    assert_off_on(content, "Colour", &["enum Colour", "case Red = 'red'"]);
}

#[test]
fn class_declaration_with_a_constructor_still_shows_the_class_hover() {
    let content = r#"<?php
class Product {
    public function __construct(private string $sku) {}
}
"#;
    let hover = assert_off_on_token(content, "class Product", "Product", &["class Product"]);
    assert!(
        !hover_text(&hover).contains("__construct"),
        "the class hover is not the constructor hover: {}",
        hover_text(&hover)
    );
    // The constructor's own name shows the method hover.
    let ctor = assert_off_on(content, "__construct", &["function __construct", "Product"]);
    assert!(hover_text(&ctor).contains("string $sku"));
}

// ─── Method, constant and enum-case declarations ────────────────────

#[test]
fn method_declaration_shows_the_method_hover() {
    let content = r#"<?php
class Calculator {
    /** Adds two numbers. */
    public function add(int $a, int $b): int {
        return $a + $b;
    }
    public static function create(): self {
        return new self();
    }
}
"#;
    assert_off_on_token(
        content,
        "function add",
        "add",
        &[
            "function add(int $a, int $b): int",
            "Calculator",
            "Adds two numbers.",
        ],
    );
    assert_off_on_token(
        content,
        "function create",
        "create",
        &["static function create(): self", "Calculator"],
    );
}

#[test]
fn class_constant_and_enum_case_declarations_show_their_hover() {
    let content = r#"<?php
class Status {
    /** The maximum quantity per order. */
    public const MAX_QTY = 10;
}
enum Priority: int {
    case Low = 1;
    case High = 2;
}
enum Direction {
    case North;
}
"#;
    assert_off_on(
        content,
        "MAX_QTY",
        &[
            "const MAX_QTY = 10",
            "Status",
            "The maximum quantity per order.",
        ],
    );
    assert_off_on(content, "Low", &["case Low = 1;", "Priority"]);
    assert_off_on(content, "North", &["case North;", "Direction"]);
}

#[test]
fn constant_and_method_with_the_same_spelling_keep_their_own_hover() {
    let content = r#"<?php
class Labelled {
    public const LABEL = 'a label';
    public function label(): string {
        return 'x';
    }
}
"#;
    let constant = assert_off_on(content, "LABEL", &["const LABEL = 'a label'"]);
    assert!(!hover_text(&constant).contains("function label"));
    let method = assert_off_on_token(
        content,
        "function label",
        "label",
        &["function label(): string"],
    );
    assert!(!hover_text(&method).contains("const LABEL"));
}

// ─── Function and top-level constant declarations ───────────────────

#[test]
fn function_declaration_shows_the_function_hover() {
    let content = r#"<?php
/** Adds a shipping surcharge. */
function surcharge(int $cents): int {
    return $cents + 1;
}
"#;
    assert_off_on_token(
        content,
        "function surcharge",
        "surcharge",
        &[
            "function surcharge(int $cents): int",
            "Adds a shipping surcharge.",
        ],
    );
}

#[test]
fn top_level_const_declaration_shows_the_constant_hover() {
    let content = "<?php\nconst LIMIT = 5;\n";
    assert_off_on(content, "LIMIT", &["const LIMIT = 5;"]);
}

#[test]
fn define_declaration_shows_the_constant_hover() {
    let content = "<?php\ndefine('LIMIT', 5);\n";
    assert_off_on_token(content, "'LIMIT'", "LIMIT", &["const LIMIT = 5;"]);
}

/// Every form the one `define()` recogniser accepts (D-14) is a declaration
/// the index holds, so each one hovers: a `defined() or` guard (issue 6), a
/// fully qualified and an upper-case callee (issue 8), and a ternary.
#[test]
fn every_define_form_shows_the_constant_hover() {
    for (content, token, expected) in [
        (
            "<?php\ndefined('ORX') or define('ORX', 5);\n",
            "ORX",
            "const ORX = 5;",
        ),
        (
            "<?php\n!defined('ANDX') && define('ANDX', 6);\n",
            "ANDX",
            "const ANDX = 6;",
        ),
        (
            "<?php\nnamespace App;\n\\define('GLOBAL_NAME', 20);\n",
            "GLOBAL_NAME",
            "const GLOBAL_NAME = 20;",
        ),
        ("<?php\nDEFINE('UPPER', 7);\n", "UPPER", "const UPPER = 7;"),
        (
            "<?php\n$fast ? define('MODE', 1) : null;\n",
            "MODE",
            "const MODE = 1;",
        ),
    ] {
        // The declaration's own name, not the `defined('…')` use before it.
        let call = if content.contains("DEFINE(") {
            format!("DEFINE('{token}'")
        } else {
            format!("define('{token}'")
        };
        assert_off_on_token(content, &call, token, &[expected]);
    }
}

// ─── Still suppressed with the switch on ────────────────────────────

#[test]
fn namespace_declaration_stays_suppressed_with_the_switch_on() {
    let content = "<?php\nnamespace App;\nclass Post {}\n";
    let (line, character) = inside(content, "App");
    assert!(hover_at(&on_backend(), "file:///test.php", content, line, character).is_none());
}

// ─── Declaration identity ───────────────────────────────────────────
// A declaration is looked up by the name it declares and the place it
// is written, never by re-resolving its spelling the way a usage is
// resolved, and never answered by another declaration of that name.

#[test]
fn const_and_define_with_the_same_short_name_keep_their_own_values() {
    let content = "<?php\nnamespace App;\nconst LIMIT = 2;\ndefine('LIMIT', 1);\n";
    assert_off_on_token(content, "const LIMIT", "LIMIT", &["const LIMIT = 2;"]);
    assert_off_on_token(content, "define('LIMIT'", "LIMIT", &["const LIMIT = 1;"]);
}

#[test]
fn define_with_a_qualified_name_ignores_the_current_namespace() {
    for content in [
        "<?php\nnamespace App;\ndefine('Other\\\\X', 3);\n",
        "<?php\nnamespace App;\ndefine(\"Other\\\\X\", 3);\n",
    ] {
        assert_off_on(content, "Other\\\\X", &["const Other\\X = 3;"]);
    }
}

#[test]
fn define_with_an_escaped_leading_backslash_shows_the_global_constant() {
    let content = r#"<?php
namespace App;
define('\\LIMIT', 1);
"#;
    assert_off_on(content, "LIMIT", &["const LIMIT = 1;"]);
}

/// Hover `token` written at `position` and assert it shows `own` and
/// not `other`, over exactly that token, or nothing at all. Returns
/// whether it hovered.
fn hovers_as_itself_or_not_at_all(
    backend: &Backend,
    uri: &str,
    content: &str,
    position: Position,
    token: &str,
    own: &str,
    other: &str,
) -> bool {
    let inside = Position {
        line: position.line,
        character: position.character + 1,
    };
    let Some(hover) = backend.handle_hover(uri, content, inside) else {
        return false;
    };
    let text = hover_text(&hover);
    assert!(
        text.contains(own) && !text.contains(other),
        "{token} in {uri} should show {own:?}, never {other:?}, got: {text}"
    );
    assert_eq!(hover.range, Some(token_range(position, token)));
    true
}

#[test]
fn a_second_declaration_of_a_function_or_constant_never_shows_the_first_ones_hover() {
    // One declaration per branch of a version guard. Whichever one
    // PHPantom indexes hovers as itself; the other answers nothing.
    let content = r#"<?php
if (PHP_VERSION_ID >= 80000) {
    function poly(): int { return 1; }
    define('POLY_MODE', 'first');
} else {
    function poly(): string { return ''; }
    define('POLY_MODE', 'second');
}
"#;
    let uri = "file:///test.php";
    let backend = on_backend();
    backend.update_ast(uri, content);
    for (search, token, [first, second]) in [
        ("poly(", "poly", ["poly(): int", "poly(): string"]),
        (
            "POLY_MODE",
            "POLY_MODE",
            ["const POLY_MODE = 'first';", "const POLY_MODE = 'second';"],
        ),
    ] {
        let first_hovers = hovers_as_itself_or_not_at_all(
            &backend,
            uri,
            content,
            position_of(content, search),
            token,
            first,
            second,
        );
        let second_hovers = hovers_as_itself_or_not_at_all(
            &backend,
            uri,
            content,
            position_of_last(content, search),
            token,
            second,
            first,
        );
        assert!(
            first_hovers != second_hovers,
            "exactly the indexed declaration of {token} should hover"
        );
    }
}

#[test]
fn a_declaration_in_another_file_never_answers_for_this_one() {
    let first = "<?php\nfunction shared(): int { return 1; }\nconst SHARED = 'first';\n";
    let second = "<?php\nfunction shared(): string { return ''; }\nconst SHARED = 'second';\n";
    let (first_uri, second_uri) = ("file:///first.php", "file:///second.php");
    let backend = on_backend();
    backend.update_ast(first_uri, first);
    backend.update_ast(second_uri, second);
    for (token, [own_first, own_second]) in [
        ("shared", ["shared(): int", "shared(): string"]),
        (
            "SHARED",
            ["const SHARED = 'first';", "const SHARED = 'second';"],
        ),
    ] {
        let first_hovers = hovers_as_itself_or_not_at_all(
            &backend,
            first_uri,
            first,
            position_of(first, token),
            token,
            own_first,
            own_second,
        );
        let second_hovers = hovers_as_itself_or_not_at_all(
            &backend,
            second_uri,
            second,
            position_of(second, token),
            token,
            own_second,
            own_first,
        );
        assert!(
            first_hovers != second_hovers,
            "exactly the indexed declaration of {token} should hover"
        );
    }
}

#[test]
fn a_class_declared_in_two_files_hovers_only_where_the_index_holds_it() {
    // Every name sits at the same offset in both files, so only the
    // declaring file tells the two declarations apart.
    let first = "<?php\n/** One. */\nclass Poly {\n    public const MODE = 'one';\n    public function answer(): int { return 1; }\n}\n";
    let second = "<?php\n/** Two. */\nclass Poly {\n    public const MODE = 'two';\n    public function answer(): string { return ''; }\n}\n";
    let (first_uri, second_uri) = ("file:///first.php", "file:///second.php");
    let cases = [
        ("class", "One.", "Two."),
        ("Poly", "One.", "Two."),
        ("MODE", "const MODE = 'one';", "const MODE = 'two';"),
        ("answer", "answer(): int", "answer(): string"),
    ];
    // Both orders: whichever file is hovered first warms the
    // resolved-class cache for the name.
    for second_file_first in [false, true] {
        let backend = on_backend();
        backend.update_ast(first_uri, first);
        backend.update_ast(second_uri, second);
        for (token, own_first, own_second) in cases {
            let first_hover = |backend: &Backend| {
                hovers_as_itself_or_not_at_all(
                    backend,
                    first_uri,
                    first,
                    position_of(first, token),
                    token,
                    own_first,
                    own_second,
                )
            };
            let second_hover = |backend: &Backend| {
                hovers_as_itself_or_not_at_all(
                    backend,
                    second_uri,
                    second,
                    position_of(second, token),
                    token,
                    own_second,
                    own_first,
                )
            };
            let (first_hovers, second_hovers) = if second_file_first {
                let second_hovers = second_hover(&backend);
                (first_hover(&backend), second_hovers)
            } else {
                (first_hover(&backend), second_hover(&backend))
            };
            assert!(
                first_hovers != second_hovers,
                "exactly the indexed declaration of {token} should hover"
            );
        }
    }
}

#[test]
fn anonymous_class_members_in_two_files_each_hover_as_themselves() {
    // Migration-style files: the anonymous class opens at the same
    // offset in both, so only the file tells the two apart.
    let base = "<?php\nabstract class Migration {\n    /** Run the migrations. */\n    abstract public function up(): void;\n}\n";
    let first = "<?php\nreturn new class extends Migration {\n    public const STEP = 'one';\n    /** @inheritDoc */\n    public function up(): void {}\n};\n";
    let second = "<?php\nreturn new class extends Migration {\n    public const STEP = 'two';\n    /** @inheritDoc */\n    public function up(): void {}\n};\n";
    let (first_uri, second_uri) = ("file:///first.php", "file:///second.php");
    for second_file_first in [false, true] {
        let backend = on_backend();
        backend.update_ast("file:///base.php", base);
        backend.update_ast(first_uri, first);
        backend.update_ast(second_uri, second);
        let mut files = [
            (
                first_uri,
                first,
                "const STEP = 'one';",
                "const STEP = 'two';",
            ),
            (
                second_uri,
                second,
                "const STEP = 'two';",
                "const STEP = 'one';",
            ),
        ];
        if second_file_first {
            files.reverse();
        }
        for (uri, content, own, other) in files {
            for (token, own, other) in [
                ("STEP", own, other),
                ("up", "Run the migrations.", "abstract public function"),
            ] {
                assert!(
                    hovers_as_itself_or_not_at_all(
                        &backend,
                        uri,
                        content,
                        position_of(content, token),
                        token,
                        own,
                        other,
                    ),
                    "{token} in {uri} should hover"
                );
            }
        }
    }
}

#[tokio::test]
async fn a_stub_path_override_suppresses_both_declarations_and_the_stub_alone_hovers() {
    // A `[stubs]` path wins over the index for the class loader and for
    // calls, but not for the provenance line or body inference, so a
    // name it overrides has no declaration every lookup agrees on.
    // `Overridden` and `customHelper` sit at the same offsets in both
    // files; `Shifted` does not. What only the stub declares hovers.
    let project = "<?php\n/** Live. */\nclass Overridden {\n    public const MODE = 'live';\n    public function answer(): int { return 1; }\n}\nclass Shifted {\n    public function size(): int { return 1; }\n}\nfunction customHelper(): int { return 1; }\nfunction run(): void {\n    customHelper();\n    stubOnly();\n}\n";
    let stub = "<?php\n/** Stub. */\nclass Overridden {\n    public const MODE = 'stub';\n    public function answer(): string {}\n}\n/**\n * Shifted down.\n */\nclass Shifted {\n    public function size(): string {}\n}\nfunction customHelper(): string {}\n/** Only the stub declares this. */\nclass OnlyInStub {}\n/** Declared by the stub alone. */\nfunction stubOnly(): string {}\n";
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("z-stubs")).unwrap();
    std::fs::write(
        root.join(".phpantom.toml"),
        "[stubs]\npaths = [\"z-stubs\"]\n",
    )
    .unwrap();
    std::fs::write(root.join("a-project.php"), project).unwrap();
    std::fs::write(root.join("z-stubs/override.php"), stub).unwrap();
    let backend = Backend::new_test_with_workspace(root.to_path_buf(), Vec::new())
        .with_hover_on_declarations(true);
    backend
        .initialize(InitializeParams::default())
        .await
        .unwrap();
    backend.initialized(InitializedParams {}).await;
    let project_uri = Uri::from_file_path(root.join("a-project.php")).unwrap();
    let stub_uri = Uri::from_file_path(root.join("z-stubs/override.php")).unwrap();
    open_php(&backend, &project_uri, project).await;
    open_php(&backend, &stub_uri, stub).await;
    let hover = |uri: &Uri, content: &str, search: &str, token: &str| {
        let (line, character) = inside_token(content, search, token);
        backend.handle_hover(uri.as_str(), content, Position { line, character })
    };

    // The project's call resolves to the stub's declaration.
    let call = hover(&project_uri, project, "    customHelper();", "customHelper")
        .expect("the call should hover");
    assert!(
        hover_text(&call).contains("customHelper(): string"),
        "the call should resolve to the stub: {}",
        hover_text(&call)
    );

    for (uri, content, origin) in [
        (&project_uri, project, "project"),
        (&stub_uri, stub, "stub"),
    ] {
        for (search, token) in [
            ("class Overridden", "class"),
            ("class Overridden", "Overridden"),
            ("const MODE", "MODE"),
            ("function answer", "answer"),
            ("class Shifted", "Shifted"),
            ("function size", "size"),
            ("function customHelper", "customHelper"),
        ] {
            let found = hover(uri, content, search, token);
            assert!(
                found.is_none(),
                "{token} in the {origin} file should answer nothing, got: {found:?}"
            );
        }
    }

    let class = hover(&stub_uri, stub, "class OnlyInStub", "OnlyInStub")
        .expect("a class only the stub declares should hover");
    assert!(hover_text(&class).contains("Only the stub declares this."));
    let declaration = hover(&stub_uri, stub, "function stubOnly", "stubOnly")
        .expect("a function only the stub declares should hover");
    let call =
        hover(&project_uri, project, "    stubOnly();", "stubOnly").expect("its call should hover");
    assert_eq!(hover_text(&declaration), hover_text(&call));
    assert!(hover_text(&declaration).contains("Declared by the stub alone."));
}

#[tokio::test]
async fn a_stub_path_constant_override_suppresses_the_declaration_before_the_stub_loads() {
    // A usage loads the stub path's constant into the index, so until
    // one runs the index holds only the project's declarations. They
    // must answer nothing from the first request, under the exact name
    // each declares: a global `const`, a `define()` and a namespaced
    // `const`. What only the project declares still hovers.
    let project = "<?php\nconst CUSTOM_MODE = 'live';\ndefine('DEFINED_MODE', 'live');\nconst PROJECT_ONLY = 'live';\necho CUSTOM_MODE, DEFINED_MODE;\n";
    let project_ns = "<?php\nnamespace App;\nconst MODE = 'live';\necho MODE;\n";
    let stub = "<?php\nconst CUSTOM_MODE = 'stub';\ndefine('DEFINED_MODE', 'stub');\n";
    let stub_ns = "<?php\nnamespace App;\nconst MODE = 'stub';\n";
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("z-stubs")).unwrap();
    std::fs::write(
        root.join(".phpantom.toml"),
        "[stubs]\npaths = [\"z-stubs\"]\n",
    )
    .unwrap();
    std::fs::write(root.join("a-project.php"), project).unwrap();
    std::fs::write(root.join("b-app.php"), project_ns).unwrap();
    std::fs::write(root.join("z-stubs/override.php"), stub).unwrap();
    std::fs::write(root.join("z-stubs/app.php"), stub_ns).unwrap();
    let backend = Backend::new_test_with_workspace(root.to_path_buf(), Vec::new())
        .with_hover_on_declarations(true);
    backend
        .initialize(InitializeParams::default())
        .await
        .unwrap();
    backend.initialized(InitializedParams {}).await;
    let project_uri = Uri::from_file_path(root.join("a-project.php")).unwrap();
    let project_ns_uri = Uri::from_file_path(root.join("b-app.php")).unwrap();
    let stub_uri = Uri::from_file_path(root.join("z-stubs/override.php")).unwrap();
    let stub_ns_uri = Uri::from_file_path(root.join("z-stubs/app.php")).unwrap();
    open_php(&backend, &project_uri, project).await;
    open_php(&backend, &project_ns_uri, project_ns).await;
    let hover = |uri: &Uri, content: &str, search: &str, token: &str| {
        let (line, character) = inside_token(content, &format!("{search}{token}"), token);
        backend.handle_hover(uri.as_str(), content, Position { line, character })
    };
    let declarations = [
        (&project_uri, project, "const ", "CUSTOM_MODE"),
        (&project_uri, project, "define('", "DEFINED_MODE"),
        (&project_ns_uri, project_ns, "const ", "MODE"),
    ];
    let usages = [
        (&project_uri, project, "echo ", "CUSTOM_MODE"),
        (&project_uri, project, "CUSTOM_MODE, ", "DEFINED_MODE"),
        (&project_ns_uri, project_ns, "echo ", "MODE"),
    ];
    let assert_suppressed = |when: &str| {
        for (uri, content, search, token) in declarations {
            let found = hover(uri, content, search, token);
            assert!(
                found.is_none(),
                "{token} in the project {when} should answer nothing, got: {found:?}"
            );
        }
    };

    assert_suppressed("before the stub loads");
    let only = hover(&project_uri, project, "const ", "PROJECT_ONLY")
        .expect("a constant only the project declares should hover");
    assert!(hover_text(&only).contains("const PROJECT_ONLY = 'live';"));

    let mut usage_texts = Vec::new();
    for (uri, content, search, token) in usages {
        let usage = hover(uri, content, search, token).expect("the usage should hover");
        assert!(
            hover_text(&usage).contains("'stub'"),
            "{token} should resolve to the stub: {}",
            hover_text(&usage)
        );
        usage_texts.push(hover_text(&usage).to_owned());
    }
    assert_suppressed("after the stub loads");

    open_php(&backend, &stub_uri, stub).await;
    open_php(&backend, &stub_ns_uri, stub_ns).await;
    let stub_declarations = [
        (&stub_uri, stub, "const ", "CUSTOM_MODE"),
        (&stub_uri, stub, "define('", "DEFINED_MODE"),
        (&stub_ns_uri, stub_ns, "const ", "MODE"),
    ];
    for ((uri, content, search, token), usage_text) in
        stub_declarations.into_iter().zip(usage_texts)
    {
        let declaration = hover(uri, content, search, token)
            .unwrap_or_else(|| panic!("{token} in the stub should hover"));
        assert_eq!(hover_text(&declaration), usage_text);
    }
}

#[tokio::test]
async fn an_untyped_function_declared_in_two_files_infers_only_where_the_index_holds_it() {
    // The bodies differ, the name offsets do not.
    let first = "<?php\nfunction sharedUntyped() { return 123; }\n";
    let second = "<?php\nfunction sharedUntyped() { return 'two'; }\n";
    let (first_uri, second_uri) = ("file:///first.php", "file:///second.php");
    let backend = on_backend();
    open_php_str(&backend, first_uri, first).await;
    open_php_str(&backend, second_uri, second).await;
    let mut hovered = Vec::new();
    for (uri, content, own, other) in [
        (first_uri, first, "123", "two"),
        (second_uri, second, "two", "123"),
    ] {
        if hovers_as_itself_or_not_at_all(
            &backend,
            uri,
            content,
            position_of(content, "sharedUntyped"),
            "sharedUntyped",
            own,
            other,
        ) {
            hovered.push(uri);
        }
    }
    assert_eq!(
        hovered.len(),
        1,
        "exactly the indexed declaration should hover, got: {hovered:?}"
    );
    let content = if hovered[0] == first_uri {
        first
    } else {
        second
    };
    let (line, character) = inside(content, "sharedUntyped");
    let hover = backend
        .handle_hover(hovered[0], content, Position { line, character })
        .expect("the indexed declaration should hover");
    assert!(
        hover_text(&hover).contains("*(inferred)*"),
        "should show its own inferred return: {}",
        hover_text(&hover)
    );
}

#[test]
fn namespaced_function_and_constant_shadow_global_ones() {
    let content = r#"<?php
namespace App;
function strlen(): int {
    return 0;
}
const DIRECTORY_SEPARATOR = 'namespaced';
"#;
    let function = assert_off_on_token(
        content,
        "function strlen",
        "strlen",
        &["function strlen(): int", "namespace App;"],
    );
    assert!(
        !hover_text(&function).contains("string $string"),
        "should not show the global strlen signature: {}",
        hover_text(&function)
    );
    assert_off_on(
        content,
        "DIRECTORY_SEPARATOR",
        &["const DIRECTORY_SEPARATOR = 'namespaced';"],
    );
}

#[test]
fn same_short_class_name_in_two_namespace_blocks_resolves_per_block() {
    let content = r#"<?php
namespace Demo {
    class Builder {}
}
namespace Other {
    class Builder {}
}
"#;
    let first = assert_off_on(content, "Builder", &["namespace Demo;"]);
    assert!(!hover_text(&first).contains("namespace Other;"));
    let second =
        hover_inside_last(&on_backend(), content, "Builder").expect("second Builder should hover");
    assert!(
        hover_text(&second).contains("namespace Other;"),
        "second block: {}",
        hover_text(&second)
    );
    assert_eq!(
        second.range,
        Some(token_range(position_of_last(content, "Builder"), "Builder"))
    );
}

// ─── Inherited documentation ────────────────────────────────────────

#[test]
fn method_declaration_shows_inherited_documentation() {
    let content = r#"<?php
class User {}
class Repository {
    /**
     * Loads the users.
     * @return list<User>
     */
    public function items(): array {
        return [];
    }
}
class UserRepository extends Repository {
    /** @inheritDoc */
    final public function items(): array {
        return [];
    }
}
"#;
    assert_off_on_token(
        content,
        "final public function items",
        "items",
        &[
            // The declaring owner, the native signature, and the
            // parent's return type and description.
            "UserRepository",
            "function items(): array",
            "list<User>",
            "Loads the users.",
        ],
    );
}

// ─── Function preparation shared with usage hover ───────────────────

#[tokio::test]
async fn function_declaration_hover_matches_call_hover_for_inferred_return() {
    let content = r#"<?php
class User {}
function makeUser() {
    return new User();
}
function run(): void {
    $user = makeUser();
}
"#;
    let backend = on_backend();
    let uri = Uri::parse("file:///test.php").unwrap();
    open_php(&backend, &uri, content).await;
    let (line, character) = inside_token(content, "function makeUser", "makeUser");
    let declaration = hover_response(&backend, &uri, line, character)
        .await
        .expect("declaration should hover");
    assert_eq!(
        declaration.range,
        Some(range_of_token(content, "function makeUser", "makeUser"))
    );
    let (line, character) = inside_token(content, "$user = makeUser();", "makeUser");
    let call = hover_response(&backend, &uri, line, character)
        .await
        .expect("call should hover");
    assert_eq!(hover_text(&declaration), hover_text(&call));
    assert!(
        hover_text(&declaration).contains("`User` *(inferred)*"),
        "should show the inferred return: {}",
        hover_text(&declaration)
    );
}

#[test]
fn function_declaration_hover_matches_call_hover_for_see_links() {
    let content = r#"<?php
class OtherClass {
    public function method(): void {}
}
/** @see OtherClass::method() */
function helper(): void {}
helper();
"#;
    let declaration = assert_off_on_token(content, "function helper", "helper", &[]);
    let call = hover_inside_last(&on_backend(), content, "helper").expect("call should hover");
    assert_eq!(hover_text(&declaration), hover_text(&call));
    assert!(
        hover_text(&declaration).contains("[`OtherClass::method()`](file:///test.php#L3)"),
        "should show the resolved @see link: {}",
        hover_text(&declaration)
    );
}

// ─── Unaffected usage behaviour ─────────────────────────────────────

#[test]
fn hover_after_new_still_shows_the_constructor_with_the_switch_on() {
    let content = r#"<?php
class Product {
    public function __construct(private string $sku) {}
}
$product = new Product('x');
"#;
    let (line, character) = inside_token(content, "new Product", "Product");
    let off = hover_on_text(&create_test_backend(), content, line, character);
    let on = hover_on_text(&on_backend(), content, line, character);
    assert_eq!(off, on, "usage hover must not change with the switch");
    let on = on.expect("the constructor hover should still show");
    assert!(on.contains("__construct"), "constructor: {on}");
    assert!(on.contains("string $sku"), "constructor: {on}");
}

#[test]
fn usage_hovers_are_identical_with_the_switch_on_and_off() {
    let content = r#"<?php
class Calculator {
    public function add(int $a, int $b): int {
        return $a + $b;
    }
    public function run(): void {
        $this->add(1, 2);
    }
}
"#;
    let (line, character) = inside_last(content, "add(1, 2)");
    let off = hover_on_text(&create_test_backend(), content, line, character);
    let on = hover_on_text(&on_backend(), content, line, character);
    assert_eq!(off, on);
    assert!(on.expect("call should hover").contains("function add"));
}

// ─── Non-ASCII and truncated input ──────────────────────────────────

#[test]
fn declaration_after_non_ascii_text_on_the_same_line_hovers() {
    let content = "<?php\necho 'café'; class AfterUnicode { public function label(): string { return 'x'; } }\n";
    assert_off_on_token(
        content,
        "class AfterUnicode",
        "AfterUnicode",
        &["class AfterUnicode"],
    );
    assert_off_on_token(
        content,
        "function label",
        "label",
        &["function label(): string"],
    );
}

#[test]
fn truncated_declaration_at_the_end_of_a_file_never_panics() {
    for (content, name) in [
        ("<?php\nclass Truncated", "Truncated"),
        ("<?php\nfunction truncated(", "truncated"),
    ] {
        let (line, character) = inside_last(content, name);
        let hover = hover_at(&on_backend(), "file:///test.php", content, line, character);
        if let Some(hover) = hover {
            assert!(
                hover_text(&hover).contains("Truncated")
                    || hover_text(&hover).contains("truncated"),
                "a truncated declaration hovers as itself or not at all: {}",
                hover_text(&hover)
            );
        }
    }
}

// ─── Location round trip ────────────────────────────────────────────
// Agents reach declarations through workspace symbols, document
// symbols, definition and implementation, so the positions those
// return are fed unchanged into hover here.

const ROUND_TRIP: &str = r#"<?php
class RoundTrip {
    public const LIMIT = 3;
    public function title(): string {
        return 'x';
    }
}
function roundTripFn(): int {
    return 1;
}
const ROUND_TRIP_LIMIT = 9;
$trip = new RoundTrip();
$trip->title();
echo RoundTrip::LIMIT;
echo roundTripFn();
echo ROUND_TRIP_LIMIT;
"#;

fn assert_hover_at(
    backend: &Backend,
    content: &str,
    position: Position,
    fragment: &str,
    what: &str,
) {
    let hover = hover_at(
        backend,
        "file:///test.php",
        content,
        position.line,
        position.character,
    )
    .unwrap_or_else(|| panic!("{what} at {position:?} should hover"));
    assert!(
        hover_text(&hover).contains(fragment),
        "{what} at {position:?} should contain {fragment:?}, got: {}",
        hover_text(&hover)
    );
}

#[test]
#[allow(deprecated)] // SymbolInformation::deprecated is deprecated in the LSP types crate
fn workspace_symbol_positions_hover_as_declarations() {
    let backend = on_backend();
    let uri = "file:///test.php";
    backend
        .open_files()
        .write()
        .insert(uri.to_string(), Arc::new(ROUND_TRIP.to_string()));
    backend.update_ast(uri, ROUND_TRIP);
    let symbols = backend.handle_workspace_symbol("").unwrap_or_default();
    for (name, fragment) in [
        ("RoundTrip", "class RoundTrip"),
        ("RoundTrip::title", "function title(): string"),
        ("RoundTrip::LIMIT", "const LIMIT = 3"),
        ("roundTripFn", "function roundTripFn(): int"),
        ("ROUND_TRIP_LIMIT", "const ROUND_TRIP_LIMIT = 9;"),
    ] {
        let symbol = symbols
            .iter()
            .find(|symbol| symbol.base_symbol_information.name == name)
            .unwrap_or_else(|| panic!("workspace symbol {name} should exist"));
        assert_hover_at(
            &backend,
            ROUND_TRIP,
            symbol.location.range.start,
            fragment,
            "workspace symbol",
        );
    }
}

fn flatten_document_symbols(symbols: &[DocumentSymbol], out: &mut Vec<DocumentSymbol>) {
    for symbol in symbols {
        out.push(symbol.clone());
        if let Some(children) = &symbol.children {
            flatten_document_symbols(children, out);
        }
    }
}

#[test]
fn document_symbol_positions_hover_as_declarations() {
    let backend = on_backend();
    let uri = "file:///test.php";
    backend.update_ast(uri, ROUND_TRIP);
    let response = backend
        .handle_document_symbol(uri, ROUND_TRIP)
        .expect("document symbols should exist");
    let DocumentSymbolResponse::DocumentSymbolList(top) = response else {
        panic!("expected nested document symbols");
    };
    let mut symbols = Vec::new();
    flatten_document_symbols(&top, &mut symbols);
    for (name, fragment) in [
        ("RoundTrip", "class RoundTrip"),
        ("title", "function title(): string"),
        ("LIMIT", "const LIMIT = 3"),
        ("roundTripFn", "function roundTripFn(): int"),
        ("ROUND_TRIP_LIMIT", "const ROUND_TRIP_LIMIT = 9;"),
    ] {
        let symbol = symbols
            .iter()
            .find(|symbol| symbol.name == name)
            .unwrap_or_else(|| panic!("document symbol {name} should exist"));
        assert_hover_at(
            &backend,
            ROUND_TRIP,
            symbol.range.start,
            fragment,
            "document symbol range",
        );
        assert_hover_at(
            &backend,
            ROUND_TRIP,
            symbol.selection_range.start,
            fragment,
            "document symbol selection range",
        );
    }
}

#[tokio::test]
async fn definition_positions_hover_as_declarations() {
    let backend = on_backend();
    let uri = Uri::parse("file:///test.php").unwrap();
    open_php(&backend, &uri, ROUND_TRIP).await;
    for (search, token, fragment) in [
        ("new RoundTrip", "RoundTrip", "class RoundTrip"),
        ("$trip->title()", "title", "function title(): string"),
        ("RoundTrip::LIMIT;", "LIMIT", "const LIMIT = 3"),
        (
            "roundTripFn();",
            "roundTripFn",
            "function roundTripFn(): int",
        ),
        (
            "echo ROUND_TRIP_LIMIT;",
            "ROUND_TRIP_LIMIT",
            "const ROUND_TRIP_LIMIT = 9;",
        ),
    ] {
        let (line, character) = inside_token(ROUND_TRIP, search, token);
        let locations =
            definition_locations(goto_definition_at(&backend, &uri, line, character).await);
        let location = locations
            .first()
            .unwrap_or_else(|| panic!("definition of {token} should exist"));
        let hover = hover_response(
            &backend,
            &uri,
            location.range.start.line,
            location.range.start.character,
        )
        .await
        .unwrap_or_else(|| panic!("definition position of {token} should hover"));
        assert!(
            hover_text(&hover).contains(fragment),
            "definition position of {token} should contain {fragment:?}, got: {}",
            hover_text(&hover)
        );
    }
}

async fn implementation_locations(
    backend: &Backend,
    uri: &Uri,
    line: u32,
    character: u32,
) -> Vec<Location> {
    let params = ImplementationParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: Position { line, character },
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    match backend.goto_implementation(params).await.unwrap() {
        Some(ImplementationResponse::Definition(Definition::Location(location))) => {
            vec![location]
        }
        Some(ImplementationResponse::Definition(Definition::LocationList(locations))) => locations,
        Some(ImplementationResponse::DefinitionLinkList(links)) => links
            .into_iter()
            .map(|link| Location {
                uri: link.target_uri,
                range: link.target_selection_range,
            })
            .collect(),
        None => Vec::new(),
    }
}

#[tokio::test]
async fn implementation_positions_hover_as_declarations() {
    let content = r#"<?php
interface RenderableContract {
    public function render(): string;
}
class HtmlView implements RenderableContract {
    public function render(): string {
        return '';
    }
}
"#;
    let backend = on_backend();
    let uri = Uri::parse("file:///test.php").unwrap();
    open_php(&backend, &uri, content).await;

    let (line, character) = inside_token(
        content,
        "interface RenderableContract",
        "RenderableContract",
    );
    let locations = implementation_locations(&backend, &uri, line, character).await;
    assert!(
        !locations.is_empty(),
        "the interface should have an implementation"
    );
    let mut saw_class_hover = false;
    for location in &locations {
        if let Some(hover) = hover_response(
            &backend,
            &uri,
            location.range.start.line,
            location.range.start.character,
        )
        .await
            && hover_text(&hover).contains("HtmlView")
        {
            saw_class_hover = true;
        }
    }
    assert!(
        saw_class_hover,
        "an implementation position should hover as the HtmlView declaration"
    );

    let (line, character) = inside_token(content, "function render", "render");
    let locations = implementation_locations(&backend, &uri, line, character).await;
    assert!(
        !locations.is_empty(),
        "the interface method should have an implementation"
    );
    let mut saw_method_hover = false;
    for location in &locations {
        if let Some(hover) = hover_response(
            &backend,
            &uri,
            location.range.start.line,
            location.range.start.character,
        )
        .await
            && hover_text(&hover).contains("function render(): string")
        {
            saw_method_hover = true;
        }
    }
    assert!(
        saw_method_hover,
        "an implementation position should hover as the render() declaration"
    );
}
