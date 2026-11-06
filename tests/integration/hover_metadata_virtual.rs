//! Virtual members (`@property`, `@method`) against real members and each
//! other (D-05b, D-05c, D-07 to D-10, D-16). Types are PHPStan 2.3.1's;
//! texts follow PhpStorm 2025.2.2.

use crate::common::{
    assert_var_type, completion_response, create_test_backend, hover_markdown, offset_position,
    open_php,
};
use tower_lsp_server::gen_lsp_types::*;

const D5_VIRTUAL: &str = r#"<?php
/** @property list<string> $items */
class OwnReal {
    private array $items = [];
    public function show(): void { $__d1 = $this->items; }
}
class Inventory { public array $items = []; public static array $stock = []; public int $value = 1; }
/**
 * @property list<string> $items
 * @property list<string> $value
 */
class Shop extends Inventory {}
$__d2 = (new Shop())->items;
$__d3 = (new Shop())->value;
/** @property list<string> $stock */
interface DocumentsStock {}
class Store extends Inventory implements DocumentsStock {}
$__d4 = Store::$stock;
/**
 * @property array $tags
 * @phpstan-property list<string> $tags
 */
class TagSource { public function __get(string $n): mixed { return null; } }
class TagChild extends TagSource {}
$__d5 = (new TagSource())->tags;
$__d6 = (new TagChild())->tags;
/**
 * @method array names()
 * @phpstan-method list<string> Names()
 */
class Methods { public function __call(string $n, array $a): mixed { return null; } }
$__d7 = (new Methods())->names();
/** @property string $name */
class ParentDoc { public function __get(string $n): mixed { return null; } }
/** @property non-empty-string $name */
class ChildDoc extends ParentDoc {}
$__d8 = (new ChildDoc())->name;
"#;

const D6_EXTRA: &str = r#"<?php
/**
 * @method array names()
 * @phpstan-method list<string> names()
 */
class SameCase { public function __call(string $n, array $a): mixed { return null; } }
$__d1 = (new SameCase())->names();
/**
 * @method list<string> names()
 * @method array Names()
 */
class TwoPlain { public function __call(string $n, array $a): mixed { return null; } }
$__d2 = (new TwoPlain())->names();
if (PHP_VERSION_ID >= 80000) { define('MODE', 'new'); } else { define('MODE', 'old'); }
$__d3 = MODE;
defined('ORX') or define('ORX', 5);
$__d4 = ORX;
\define('FQ_NAME', 20);
$__d5 = FQ_NAME;
"#;

const A2_CHILD_TAG_WITHOUT_TEXT: &str = r#"<?php
/**
 * @property string $name The parent name.
 * @method int count() Counts the parent.
 */
class ParentDoc {}
/**
 * @property non-empty-string $name
 * @method int count()
 */
class ChildDoc extends ParentDoc {}
$child = new ChildDoc();
echo $child->name;
$child->count();
"#;

const A7_CONSUMERS_AGREE: &str = r#"<?php
class Inventory { /** The stock. */ public array $items = []; }
/** @property list<string> $items */
class Shop extends Inventory {}
$shop = new Shop();
echo $shop->items;
foreach ($shop->items as $item) { echo $item; }
$shop->
"#;

/// A real property wins over a `@property` tag on its own class; a child's
/// tag narrows an inherited real property.
///
/// Spec: COMB-004.
#[test]
fn d05b_real_property_wins_on_own_class_and_child_tag_narrows_inherited() {
    for (var, expected) in [("$__d1", "array"), ("$__d2", "list<string>")] {
        assert_var_type(D5_VIRTUAL, var, expected);
    }
}

/// `@phpstan-method` beats `@method` of the same name, and of two plain
/// `@method` tags whose names differ only in case the first wins.
///
/// Spec: METH-007, CLS-011.
#[test]
fn d05c_method_tags_by_case_and_vendor() {
    for (var, expected) in [("$__d1", "list<string>"), ("$__d2", "list<string>")] {
        assert_var_type(D6_EXTRA, var, expected);
    }
}

/// A `@phpstan-property` on the declaring class wins over its `@property`,
/// and a child's `@property` narrows the parent's.
///
/// Spec: PROP-016, INH-020.
#[test]
fn d09_own_docblock_and_child_tags() {
    for (var, expected) in [("$__d5", "list<string>"), ("$__d8", "non-empty-string")] {
        assert_var_type(D5_VIRTUAL, var, expected);
    }
}

/// Of `@method` and `@phpstan-method` tags whose names differ only in case,
/// the first tag wins.
#[test]
fn d16_method_names_differing_in_case_first_tag_wins() {
    assert_var_type(D5_VIRTUAL, "$__d7", "array");
}

/// A child's `@property` / `@method` tag without text shows its own type
/// and no text, not the parent tag's text.
#[test]
fn d10_child_tag_without_text_shows_no_text() {
    for (needle, ty, parent_text) in [
        ("->name", "`non-empty-string`", "The parent name."),
        ("->count", "`int`", "Counts the parent."),
    ] {
        let markdown = hover_markdown(A2_CHILD_TAG_WITHOUT_TEXT, needle, 2, 0)
            .unwrap_or_else(|| panic!("expected a hover on {needle}"));
        assert!(markdown.contains(ty), "{needle}: type {ty}: {markdown}");
        assert!(
            !markdown.contains(parent_text),
            "{needle}: no parent text: {markdown}"
        );
    }
}

/// Hover, foreach inference and completion agree on the type of an
/// inherited real property narrowed by a child's `@property`.
#[tokio::test]
async fn consumers_agree_on_a_narrowed_inherited_property() {
    let content = A7_CONSUMERS_AGREE;
    let markdown = hover_markdown(content, "->items", 2, 0).expect("expected a hover on ->items");
    assert!(markdown.contains("`list<string>`"), "->items: {markdown}");
    let markdown = hover_markdown(content, "$item;", 1, 0).expect("expected a hover on $item");
    assert!(markdown.contains("$item = string\n"), "$item: {markdown}");

    let backend = create_test_backend();
    let uri = Uri::parse("file:///test.php").unwrap();
    open_php(&backend, &uri, content).await;
    let offset = content.rfind("$shop->\n").unwrap() + "$shop->".len();
    let position = offset_position(content, offset);
    let items =
        match completion_response(&backend, &uri, position.line, position.character, None).await {
            Some(CompletionResponse::CompletionItemList(items)) => items,
            Some(CompletionResponse::CompletionList(list)) => list.items,
            None => panic!("expected completion items after $shop->"),
        };
    let item = items
        .iter()
        .find(|item| item.label == "items")
        .unwrap_or_else(|| panic!("expected an `items` completion: {items:?}"));
    assert_eq!(item.detail.as_deref(), Some("list<string>"), "{item:?}");
}

/// A `@property` tag on an interface never retypes a static property.
///
/// Spec: INH-023.
#[test]
#[ignore = "issue 19 / D-08: an interface @property tag retypes an inherited static property"]
fn issue_19_interface_tag_never_retypes_a_static_property() {
    assert_var_type(D5_VIRTUAL, "$__d4", "array");
}

/// The inherited docblock's `@phpstan-property` wins over its `@property`
/// in a child, as on the declaring class.
#[test]
#[ignore = "issue 20 / D-09: an inherited docblock takes its first tag instead of the vendor-prefixed one"]
fn issue_20_inherited_docblock_applies_vendor_precedence() {
    assert_var_type(D5_VIRTUAL, "$__d6", "list<string>");
}

/// A child's `@property` replaces an inherited real property's type only
/// when it is a subtype of it.
///
/// Spec: INH-024.
#[test]
#[ignore = "issue 22 / D-07: a child @property retypes an inherited real property it does not narrow"]
fn issue_22_tag_narrows_only_to_a_subtype() {
    assert_var_type(D5_VIRTUAL, "$__d3", "int");
}
