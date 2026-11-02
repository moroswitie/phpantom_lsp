//! Real properties retain their declaration when PHPDoc names them again.

use crate::common::{create_test_backend, hover_last};

#[test]
fn hover_phpdoc_tag_keeps_a_real_property() {
    let backend = create_test_backend();
    for native in ["array ", "mixed ", ""] {
        let content = format!(
            r#"<?php
/** @property list<string> $items */
class Inventory {{
    /** The real items. */
    private {native}$items = [];
    public function show(): void {{ echo $this->items; }}
}}
"#
        );
        let text = hover_last(&backend, &content, "items;");
        assert!(text.contains(&format!("private {native}$items;")), "{text}");
        assert!(text.contains("The real items."), "{text}");
        assert!(!text.contains("virtual"), "{text}");
        assert!(!text.contains("list<string>"), "{text}");
    }
}

#[test]
fn hover_phpdoc_tag_narrows_an_inherited_public_property() {
    let backend = create_test_backend();
    let content = r#"<?php
class Inventory {
    /** The inherited items. */
    public array $items = [];
}
/** @property list<string> $items */
class Shop extends Inventory {}
$shop = new Shop();
echo $shop->items;
foreach ($shop->items as $item) { echo $item; }
"#;
    let text = hover_last(&backend, content, "items;");
    assert!(text.contains("public array $items;"), "{text}");
    assert!(text.contains("The inherited items."), "{text}");
    assert!(text.contains("**var** `list<string>`"), "{text}");
    assert!(!text.contains("virtual"), "{text}");
    let item = hover_last(&backend, content, "item; }");
    assert!(item.contains("$item = string"), "{item}");
}

#[test]
fn hover_phpdoc_tag_does_not_widen_an_inherited_public_property() {
    let backend = create_test_backend();
    for tag in ["array", "mixed"] {
        let content = format!(
            r#"<?php
class Inventory {{
    /** @var list<string> The inherited items. */
    public array $items = [];
}}
/** @property {tag} $items */
class Shop extends Inventory {{}}
$shop = new Shop();
echo $shop->items;
"#
        );
        let text = hover_last(&backend, &content, "items;");
        assert!(text.contains("public array $items;"), "{tag}: {text}");
        assert!(text.contains("**var** `list<string>`"), "{tag}: {text}");
        assert!(!text.contains("virtual"), "{tag}: {text}");
    }
}

#[test]
fn hover_phpdoc_tag_retypes_an_inherited_protected_property_without_replacing_it() {
    let backend = create_test_backend();
    let content = r#"<?php
class Inventory {
    /** The inherited items. */
    protected array $items = [];
}
/** @property list<string> $items */
class Shop extends Inventory {
    public function show(): void { echo $this->items; }
}
"#;
    let text = hover_last(&backend, content, "items;");
    assert!(text.contains("protected array $items;"), "{text}");
    assert!(text.contains("The inherited items."), "{text}");
    assert!(text.contains("list<string>"), "{text}");
    assert!(!text.contains("virtual"), "{text}");
}

#[test]
fn hover_phpdoc_tag_narrows_a_trait_property() {
    let backend = create_test_backend();
    let content = r#"<?php
trait HasItems {
    /** The trait items. */
    public array $items = [];
}
/** @property list<string> $items */
class Inventory {
    use HasItems;
    public function show(): void { echo $this->items; }
}
"#;
    let text = hover_last(&backend, content, "items;");
    assert!(text.contains("public array $items;"), "{text}");
    assert!(text.contains("The trait items."), "{text}");
    assert!(text.contains("**var** `list<string>`"), "{text}");
    assert!(!text.contains("virtual"), "{text}");
}

/// Hover `$subject->items` on a `new {class}()` after `classes`.
fn hover_items_of(classes: &str, class: &str) -> String {
    let backend = create_test_backend();
    let content = format!("<?php\n{classes}\n$subject = new {class}();\necho $subject->items;\n");
    hover_last(&backend, &content, "items;")
}

/// The element type `foreach` gives `$subject->items` on a `new {class}()`.
fn hover_item_of(classes: &str, class: &str) -> String {
    let backend = create_test_backend();
    let content = format!(
        "<?php\n{classes}\n$subject = new {class}();\nforeach ($subject->items as $item) {{ echo $item; }}\n"
    );
    hover_last(&backend, &content, "item; }")
}

fn assert_narrowed(text: &str, declaration: &str) {
    assert!(text.contains(declaration), "{text}");
    assert!(text.contains("**var** `list<string>`"), "{text}");
    assert!(!text.contains("virtual"), "{text}");
}

fn assert_not_narrowed(text: &str, declaration: &str) {
    assert!(text.contains(declaration), "{text}");
    assert!(!text.contains("list<string>"), "{text}");
    assert!(!text.contains("virtual"), "{text}");
}

const INVENTORY: &str = r#"
class Inventory {
    /** The inherited items. */
    public array $items = [];
}
"#;

#[test]
fn hover_parent_phpdoc_tag_narrows_a_grandparent_property() {
    let classes = format!(
        "{INVENTORY}
/** @property list<string> $items */
class Shop extends Inventory {{}}
class Outlet extends Shop {{}}
"
    );
    let text = hover_items_of(&classes, "Outlet");
    assert_narrowed(&text, "public array $items;");
    assert!(text.contains("The inherited items."), "{text}");
    let item = hover_item_of(&classes, "Outlet");
    assert!(item.contains("$item = string"), "{item}");
}

#[test]
fn hover_parent_phpdoc_tag_retypes_a_grandparent_protected_property() {
    let classes = r#"
class Inventory {
    /** The inherited items. */
    protected array $items = [];
}
/** @property list<string> $items */
class Shop extends Inventory {}
class Outlet extends Shop {
    public function show(): void { echo $this->items; }
}
"#;
    let backend = create_test_backend();
    let text = hover_last(&backend, &format!("<?php\n{classes}"), "items;");
    assert!(text.contains("protected array $items;"), "{text}");
    assert!(text.contains("list<string>"), "{text}");
    assert!(!text.contains("virtual"), "{text}");
}

#[test]
fn hover_trait_phpdoc_tag_narrows_a_parent_property() {
    let classes = format!(
        "{INVENTORY}
/** @property list<string> $items */
trait DocumentsItems {{}}
class Shop extends Inventory {{ use DocumentsItems; }}
class Outlet extends Shop {{}}
"
    );
    // The class's own trait, and a trait its parent uses.
    for class in ["Shop", "Outlet"] {
        let text = hover_items_of(&classes, class);
        assert_narrowed(&text, "public array $items;");
        let item = hover_item_of(&classes, class);
        assert!(item.contains("$item = string"), "{class}: {item}");
    }
}

#[test]
fn hover_template_bound_parent_phpdoc_tag_narrows_a_property() {
    let classes = format!(
        "{INVENTORY}
/**
 * @template T
 * @property T $items
 */
abstract class Shelf extends Inventory {{}}
/** @extends Shelf<list<string>> */
class Shop extends Shelf {{}}
"
    );
    let text = hover_items_of(&classes, "Shop");
    assert_narrowed(&text, "public array $items;");
    let item = hover_item_of(&classes, "Shop");
    assert!(item.contains("$item = string"), "{item}");
}

#[test]
fn hover_phpdoc_tag_beside_its_own_property_does_not_reach_a_child() {
    let classes = r#"
/** @property list<string> $items */
class Shop {
    /** The real items. */
    public array $items = [];
}
class Outlet extends Shop {}
"#;
    for class in ["Shop", "Outlet"] {
        let text = hover_items_of(classes, class);
        assert_not_narrowed(&text, "public array $items;");
        assert!(text.contains("The real items."), "{class}: {text}");
    }
}

#[test]
fn hover_redeclared_property_shuts_out_an_ancestor_phpdoc_tag() {
    let classes = format!(
        "{INVENTORY}
/** @property list<string> $items */
class Shop extends Inventory {{}}
class Outlet extends Shop {{
    /** The redeclared items. */
    public array $items = [];
}}
class Store extends Outlet {{}}
"
    );
    for class in ["Outlet", "Store"] {
        let text = hover_items_of(&classes, class);
        assert_not_narrowed(&text, "public array $items;");
        assert!(text.contains("The redeclared items."), "{class}: {text}");
    }
}

#[test]
fn hover_nearer_phpdoc_tag_decides_over_a_more_specific_farther_one() {
    let classes = format!(
        "{INVENTORY}
/** @property list<string> $items */
class Shop extends Inventory {{}}
/** @property array $items */
class Outlet extends Shop {{}}
class Store extends Outlet {{}}
"
    );
    for class in ["Outlet", "Store"] {
        let text = hover_items_of(&classes, class);
        assert_not_narrowed(&text, "public array $items;");
    }
}

#[test]
fn hover_phpdoc_tag_does_not_retype_a_static_property() {
    let classes = r#"
class Inventory {
    /** The shared registry. */
    public static array $items = [];
}
/** @property list<string> $items */
class Shop extends Inventory {}
class Outlet extends Shop {}
"#;
    // The class's own tag, and a tag inherited from its parent.
    for class in ["Shop", "Outlet"] {
        let backend = create_test_backend();
        let content = format!("<?php\n{classes}\necho {class}::$items;\n");
        let text = hover_last(&backend, &content, "items;");
        assert!(
            text.contains("public static array $items;"),
            "{class}: {text}"
        );
        assert!(text.contains("The shared registry."), "{class}: {text}");
        assert!(!text.contains("list<string>"), "{class}: {text}");
        assert!(!text.contains("virtual"), "{class}: {text}");
    }
}

#[test]
fn hover_interface_phpdoc_tag_narrows_a_parent_property() {
    let classes = format!(
        "{INVENTORY}
/** @property list<string> $items */
interface DocumentsItems {{}}
class Shop extends Inventory implements DocumentsItems {{}}
class Outlet extends Shop {{}}
"
    );
    for class in ["Shop", "Outlet"] {
        let text = hover_items_of(&classes, class);
        assert!(text.contains("public array $items;"), "{class}: {text}");
        assert!(text.contains("list<string>"), "{class}: {text}");
        assert!(!text.contains("virtual"), "{class}: {text}");
    }
}
