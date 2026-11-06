//! Declaration hover for properties and docblock members: ordinary
//! properties (static, hooked, and each name of a multi-property
//! declaration), constructor-promoted properties, and the names in a
//! class's `@property` / `@property-read` / `@property-write` and
//! `@method` tags.
//!
//! With the switch off each of them answers `None`, exactly as before,
//! and an ordinary parameter keeps its variable hover. With it on, a
//! property declaration answers the hover a usage of the property
//! answers (`$this->name`, `static::$name`), never a variable hover,
//! and a tag answers what a usage of its member answers, from the same
//! fully resolved class.

use std::sync::Arc;

use crate::common::{
    create_test_backend, definition_locations, goto_definition_at, hover_at, hover_response,
    hover_text, open_php, position_of, position_of_last,
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
    range_of_token_at(position_of(content, search), search, token)
}

/// [`range_of_token`] inside the last occurrence of `search`.
fn range_of_token_last(content: &str, search: &str, token: &str) -> Range {
    range_of_token_at(position_of_last(content, search), search, token)
}

fn range_of_token_at(start: Position, search: &str, token: &str) -> Range {
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
/// token containing every fragment and none of `absent`. Returns the
/// on-hover's text.
fn assert_off_on(
    content: &str,
    search: &str,
    token: &str,
    fragments: &[&str],
    absent: &[&str],
) -> String {
    assert_off_on_range(
        content,
        range_of_token(content, search, token),
        fragments,
        absent,
    )
}

fn assert_off_on_range(content: &str, range: Range, fragments: &[&str], absent: &[&str]) -> String {
    let off = hover_text_at(&create_test_backend(), content, inside(range));
    assert!(
        off.is_none(),
        "off should answer nothing at {range:?}, got: {off:?}"
    );
    let (line, character) = inside(range);
    let hover = hover_at(&on_backend(), "file:///test.php", content, line, character)
        .unwrap_or_else(|| panic!("on should hover at {range:?}"));
    let text = hover_text(&hover).to_string();
    for fragment in fragments {
        assert!(
            text.contains(fragment),
            "hover at {range:?} should contain {fragment:?}, got: {text}"
        );
    }
    for fragment in absent {
        assert!(
            !text.contains(fragment),
            "hover at {range:?} should not contain {fragment:?}, got: {text}"
        );
    }
    assert_eq!(
        hover.range,
        Some(range),
        "hover should cover exactly the token"
    );
    text
}

// ─── Properties ─────────────────────────────────────────────────────

const PROPERTIES: &str = r#"<?php
namespace App;

class Post {
    /** The post's title. */
    public string $title;
    public static int $count = 0;
    protected int $width, $height;
    public string $slug {
        get => 'post';
    }
}
"#;

#[test]
fn ordinary_property_declaration_shows_the_property_hover() {
    assert_off_on(
        PROPERTIES,
        "public string $title",
        "$title",
        &[
            "namespace App;",
            "class Post",
            "public string $title;",
            "The post's title.",
        ],
        &["$title = "],
    );
}

#[test]
fn static_property_declaration_shows_the_property_hover() {
    assert_off_on(
        PROPERTIES,
        "public static int $count",
        "$count",
        &["class Post", "public static int $count;"],
        &["$count = "],
    );
}

#[test]
fn each_name_of_a_multi_property_declaration_shows_its_own_hover() {
    assert_off_on(
        PROPERTIES,
        "int $width, $height",
        "$width",
        &["protected int $width;"],
        &["$height"],
    );
    assert_off_on(
        PROPERTIES,
        "int $width, $height",
        "$height",
        &["protected int $height;"],
        &["$width"],
    );
}

#[test]
fn hooked_property_declaration_shows_the_property_hover() {
    assert_off_on(
        PROPERTIES,
        "public string $slug",
        "$slug",
        &["class Post", "public string $slug;"],
        &["$slug = "],
    );
}

#[test]
fn property_declaration_hover_matches_the_usage_hover() {
    let content = r#"<?php
class Post {
    /** The post's title. */
    public string $title;
    public static int $count = 0;
    public function show(): void {
        echo $this->title;
        echo static::$count;
    }
}
"#;
    let backend = on_backend();
    for (declaration, usage, token) in [
        ("public string $title", "$this->title", "title"),
        ("public static int $count", "static::$count", "count"),
    ] {
        let declared = hover_text_at(
            &backend,
            content,
            inside(range_of_token(content, declaration, token)),
        )
        .unwrap_or_else(|| panic!("the declaration of {token} should hover"));
        let used = hover_text_at(
            &backend,
            content,
            inside(range_of_token(content, usage, token)),
        )
        .unwrap_or_else(|| panic!("the usage of {token} should hover"));
        assert_eq!(declared, used, "{token}");
    }
}

#[test]
fn property_declaration_shows_its_native_and_phpdoc_types() {
    let content = r#"<?php
class User {}
class Team {
    /** @var list<User> */
    public array $members = [];
}
"#;
    assert_off_on(
        content,
        "public array $members",
        "$members",
        &["public array $members;", "list<User>"],
        &[],
    );
}

#[test]
fn templated_class_property_declaration_shows_its_template_type() {
    let content = r#"<?php
/**
 * @template-covariant TItem of object
 */
class Box {
    /** @var TItem */
    public $item;
}
"#;
    assert_off_on(
        content,
        "public $item",
        "$item",
        &["class Box", "public $item;", "TItem", "template-covariant"],
        &[],
    );
}

#[test]
fn property_declaration_shows_inherited_documentation() {
    let content = r#"<?php
class Model {
    /**
     * The table the model is stored in.
     * @var non-empty-string
     */
    protected string $table = 'models';
}
class Order extends Model {
    protected string $table = 'orders';
}
"#;
    assert_off_on(
        content,
        "protected string $table = 'orders'",
        "$table",
        &[
            "class Order",
            "protected string $table;",
            "The table the model is stored in.",
            "non-empty-string",
        ],
        &[],
    );
}

// ─── Promoted constructor parameters ────────────────────────────────

const PROMOTED: &str = r#"<?php
namespace App;

class Invoice {
    public function __construct(
        private readonly int $total,
        public string $currency = 'EUR',
        int $discount = 0,
    ) {
        $total = $discount;
        echo $total;
    }
}
"#;

#[test]
fn promoted_parameter_declaration_shows_the_property_hover() {
    assert_off_on(
        PROMOTED,
        "private readonly int $total",
        "$total",
        &["namespace App;", "class Invoice", "int $total;"],
        &["$total = "],
    );
    assert_off_on(
        PROMOTED,
        "public string $currency",
        "$currency",
        &["class Invoice", "public string $currency;"],
        &["$currency = "],
    );
}

#[test]
fn ordinary_parameter_keeps_its_variable_hover_with_the_switch_on_and_off() {
    let range = range_of_token(PROMOTED, "int $discount", "$discount");
    for backend in [create_test_backend(), on_backend()] {
        let (line, character) = inside(range);
        let hover = hover_at(&backend, "file:///test.php", PROMOTED, line, character)
            .expect("an ordinary parameter hovers as a variable");
        assert!(
            hover_text(&hover).contains("$discount = int"),
            "got: {}",
            hover_text(&hover)
        );
        assert_eq!(hover.range, Some(range));
    }
}

#[test]
fn promoted_parameter_and_a_local_of_the_same_name_keep_their_own_hover() {
    let backend = on_backend();
    let parameter = hover_text_at(
        &backend,
        PROMOTED,
        inside(range_of_token(
            PROMOTED,
            "private readonly int $total",
            "$total",
        )),
    )
    .expect("the promoted parameter hovers as its property");
    assert!(parameter.contains("int $total;"), "got: {parameter}");

    let local = hover_text_at(
        &backend,
        PROMOTED,
        inside(range_of_token(PROMOTED, "echo $total", "$total")),
    )
    .expect("the local hovers as a variable");
    assert!(local.contains("$total = int"), "got: {local}");
    assert!(!local.contains("class Invoice"), "got: {local}");
}

// ─── Docblock tags ──────────────────────────────────────────────────

const TAGS: &str = r#"<?php
namespace App;

/**
 * @property string $label
 * @property-read int $views
 * @property-write bool $draft
 * @method string render(int $width)
 * @method static Post make()
 */
class Post {
    public function __get(string $name): mixed { return null; }
    public function __call(string $name, array $arguments): mixed { return null; }
    public static function __callStatic(string $name, array $arguments): mixed { return null; }
}
"#;

#[test]
fn property_tag_names_show_the_property_hover() {
    // A tag declares no native type, so the type is on the `var` line.
    for (search, token, line, var) in [
        (
            "string $label",
            "label",
            "public $label;",
            "**var** `string`",
        ),
        ("int $views", "views", "public $views;", "**var** `int`"),
        ("bool $draft", "draft", "public $draft;", "**var** `bool`"),
    ] {
        assert_off_on(
            TAGS,
            search,
            token,
            &["namespace App;", "class Post", line, var, "virtual"],
            &[],
        );
    }
}

#[test]
fn method_tag_names_show_the_method_hover() {
    assert_off_on(
        TAGS,
        "string render(int $width)",
        "render",
        &[
            "class Post",
            "public function render(int $width);",
            "**return** `string`",
            "virtual",
        ],
        &[],
    );
    assert_off_on(
        TAGS,
        "static Post make()",
        "make",
        &[
            "class Post",
            "public static function make();",
            "**return** `Post`",
            "virtual",
        ],
        &[],
    );
}

#[test]
fn tag_hover_matches_the_usage_hover() {
    let content = format!(
        "{TAGS}function show(\\App\\Post $post): void {{\n    echo $post->label;\n    echo $post->render(10);\n}}\n"
    );
    let backend = on_backend();
    for (declaration, usage, token) in [
        ("string $label", "$post->label", "label"),
        ("string render(int $width)", "$post->render", "render"),
    ] {
        let declared = hover_text_at(
            &backend,
            &content,
            inside(range_of_token(&content, declaration, token)),
        )
        .unwrap_or_else(|| panic!("the tag of {token} should hover"));
        let used = hover_text_at(
            &backend,
            &content,
            inside(range_of_token(&content, usage, token)),
        )
        .unwrap_or_else(|| panic!("the usage of {token} should hover"));
        assert_eq!(declared, used, "{token}");
    }
}

#[test]
fn a_tag_named_like_a_real_member_shows_the_real_member() {
    let content = r#"<?php
/**
 * @property int $title
 * @method int render()
 */
class Post {
    public string $title = '';
    public function render(): string { return ''; }
}
"#;
    assert_off_on(
        content,
        "int $title",
        "title",
        &["public string $title;"],
        &["virtual", "int $title"],
    );
    assert_off_on(
        content,
        "int render()",
        "render",
        &["public function render(): string;"],
        &["virtual"],
    );
}

#[test]
fn a_tag_named_like_an_inherited_real_property_shows_what_a_usage_shows() {
    let content = r#"<?php
class Model {
    public string $name = '';
}
/**
 * @property int $name
 */
class User extends Model {}
function show(User $user): void {
    echo $user->name;
}
"#;
    let backend = on_backend();
    let declared = hover_text_at(
        &backend,
        content,
        inside(range_of_token(content, "int $name", "name")),
    )
    .expect("the tag hovers");
    let used = hover_text_at(
        &backend,
        content,
        inside(range_of_token(content, "$user->name", "name")),
    )
    .expect("the usage hovers");
    assert_eq!(declared, used);
}

#[test]
fn adjacent_classes_each_hover_their_own_tags() {
    let content = r#"<?php
/**
 * @property int $shared
 * @method int build()
 */
class First {}
/**
 * @property string $shared
 * @method string build()
 */
class Second {}
"#;
    assert_off_on(
        content,
        "int $shared",
        "shared",
        &["class First", "**var** `int`"],
        &["Second"],
    );
    assert_off_on(
        content,
        "string $shared",
        "shared",
        &["class Second", "**var** `string`"],
        &["First"],
    );
    assert_off_on(
        content,
        "int build()",
        "build",
        &["class First"],
        &["Second"],
    );
    assert_off_on(
        content,
        "string build()",
        "build",
        &["class Second"],
        &["First"],
    );
}

#[test]
fn tags_in_two_namespace_blocks_hover_their_own_class() {
    let content = r#"<?php
namespace One {
    /**
     * @property int $value
     */
    class Box {}
}
namespace Two {
    /**
     * @property string $value
     */
    class Box {}
}
"#;
    assert_off_on(
        content,
        "int $value",
        "value",
        &["namespace One;", "**var** `int`"],
        &["Two"],
    );
    assert_off_on(
        content,
        "string $value",
        "value",
        &["namespace Two;", "**var** `string`"],
        &["One"],
    );
}

#[test]
fn tags_outside_the_class_docblock_stay_suppressed_with_the_switch_on() {
    // A tag in a method body above an anonymous class declares no member
    // of the enclosing class, even where that class has a real member of
    // the same name.
    let content = r#"<?php
class Outer {
    public const LABEL = 'outer';
    public function magic(): int { return 1; }
    public function make(): object {
        /**
         * @method string magic()
         * @property int $LABEL
         */
        return new class {};
    }
}
"#;
    let backend = on_backend();
    for (search, token) in [("string magic()", "magic"), ("int $LABEL", "LABEL")] {
        let hover = hover_text_at(
            &backend,
            content,
            inside(range_of_token(content, search, token)),
        );
        assert!(
            hover.is_none(),
            "the tag {token} should answer nothing, got: {hover:?}"
        );
    }
}

/// A docblock between the attributes and the `class` keyword is the
/// class's docblock (PHP's rule, spec ATTR-002), so its tags declare the
/// class's members like any other class docblock's: a tag named like a
/// real member shows the real member.
#[test]
fn a_docblock_after_the_class_attributes_declares_its_members() {
    let content = r#"<?php
#[Attribute]
/**
 * @method string attributed()
 * @property int $attributedTitle
 */
class Tagged {
    public function attributed(): string { return ''; }
}
"#;
    assert_off_on(
        content,
        "string attributed()",
        "attributed",
        &["public function attributed(): string;"],
        &["virtual"],
    );
    assert_off_on(
        content,
        "int $attributedTitle",
        "attributedTitle",
        &["**var** `int`", "class Tagged", "$attributedTitle"],
        &[],
    );
}

// ─── Declaration identity ───────────────────────────────────────────

#[test]
fn a_second_declaration_of_a_class_never_shows_the_first_ones_members() {
    // The index holds the first `Post`; the second one's property,
    // promoted parameter and tag answer nothing rather than the first
    // one's members.
    let content = r#"<?php
if (PHP_VERSION_ID >= 80000) {
    /**
     * @property int $label
     */
    class Post {
        public int $title = 1;
        public function __construct(public int $sku = 1) {}
    }
} else {
    /**
     * @property string $label
     */
    class Post {
        public string $title = '';
        public function __construct(public string $sku = '') {}
    }
}
"#;
    let backend = on_backend();
    for (search, token, first) in [
        ("public int $title", "$title", "public int $title;"),
        ("public int $sku", "$sku", "public int $sku;"),
        ("int $label", "label", "**var** `int`"),
    ] {
        let hover = hover_text_at(
            &backend,
            content,
            inside(range_of_token(content, search, token)),
        )
        .unwrap_or_else(|| panic!("the first declaration of {token} hovers"));
        assert!(hover.contains(first), "got: {hover}");
    }
    for (search, token) in [
        ("public string $title", "$title"),
        ("public string $sku", "$sku"),
        ("string $label", "label"),
    ] {
        let hover = hover_text_at(
            &backend,
            content,
            inside(range_of_token_last(content, search, token)),
        );
        assert!(
            hover.is_none(),
            "the second declaration of {token} should answer nothing, got: {hover:?}"
        );
    }
}

// ─── Location round trip ────────────────────────────────────────────

const ROUND_TRIP: &str = r#"<?php
/**
 * @property string $label
 * @property-read int $views
 * @method string render()
 * @method static RoundTrip make()
 */
class RoundTrip {
    public string $title = '';
    public static int $count = 0;
    public string $slug {
        get => 'trip';
    }

    public function __construct(public int $sku = 1) {}

    public function __get(string $name): mixed { return null; }
    public function __call(string $name, array $arguments): mixed { return null; }
    public static function __callStatic(string $name, array $arguments): mixed { return null; }
}

$trip = new RoundTrip();
echo $trip->title;
echo RoundTrip::$count;
echo $trip->slug;
echo $trip->sku;
echo $trip->label;
echo $trip->views;
echo $trip->render();
RoundTrip::make();
"#;

/// The real properties of [`ROUND_TRIP`], each with a fragment of its
/// declaration hover.
const ROUND_TRIP_PROPERTIES: [(&str, &str); 4] = [
    ("title", "public string $title;"),
    ("count", "public static int $count;"),
    ("slug", "public string $slug;"),
    ("sku", "public int $sku;"),
];

fn assert_round_trip_hover_at(backend: &Backend, position: Position, fragment: &str, what: &str) {
    let hover = hover_at(
        backend,
        "file:///test.php",
        ROUND_TRIP,
        position.line,
        position.character,
    )
    .unwrap_or_else(|| panic!("{what} at {position:?} should hover"));
    let text = hover_text(&hover);
    assert!(
        text.contains(fragment) && text.contains("class RoundTrip"),
        "{what} at {position:?} should contain {fragment:?} in class RoundTrip, got: {text}"
    );
}

#[test]
#[allow(deprecated)] // SymbolInformation::deprecated is deprecated in the LSP types crate
fn workspace_symbol_positions_of_properties_hover_as_declarations() {
    let backend = on_backend();
    let uri = "file:///test.php";
    backend
        .open_files()
        .write()
        .insert(uri.to_string(), Arc::new(ROUND_TRIP.to_string()));
    backend.update_ast(uri, ROUND_TRIP);
    let symbols = backend.handle_workspace_symbol("").unwrap_or_default();
    for (name, fragment) in ROUND_TRIP_PROPERTIES {
        let symbol = symbols
            .iter()
            .find(|symbol| symbol.base_symbol_information.name == format!("RoundTrip::${name}"))
            .unwrap_or_else(|| panic!("workspace symbol for {name} should exist"));
        assert_round_trip_hover_at(
            &backend,
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
fn document_symbol_positions_of_properties_hover_as_declarations() {
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
    for (name, fragment) in ROUND_TRIP_PROPERTIES {
        let symbol = symbols
            .iter()
            .find(|symbol| symbol.name == format!("${name}"))
            .unwrap_or_else(|| panic!("document symbol for {name} should exist"));
        assert_round_trip_hover_at(
            &backend,
            symbol.range.start,
            fragment,
            "document symbol range",
        );
        assert_round_trip_hover_at(
            &backend,
            symbol.selection_range.start,
            fragment,
            "document symbol selection range",
        );
    }
}

#[tokio::test]
async fn definition_positions_of_usages_hover_as_declarations() {
    let backend = on_backend();
    let uri = Uri::parse("file:///test.php").unwrap();
    open_php(&backend, &uri, ROUND_TRIP).await;
    for (usage, token, fragment) in [
        ("$trip->title", "title", "public string $title;"),
        ("RoundTrip::$count", "count", "public static int $count;"),
        ("$trip->slug", "slug", "public string $slug;"),
        ("$trip->sku", "sku", "public int $sku;"),
        ("$trip->label", "label", "public $label;"),
        ("$trip->views", "views", "public $views;"),
        ("$trip->render", "render", "function render()"),
        ("RoundTrip::make", "make", "public static function make()"),
    ] {
        let (line, character) = inside(range_of_token(ROUND_TRIP, usage, token));
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
        assert!(
            hover_text(&hover).contains("class RoundTrip"),
            "definition position of {token} should name its class, got: {}",
            hover_text(&hover)
        );
    }
}
