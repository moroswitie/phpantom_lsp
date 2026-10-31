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
fn hover_phpdoc_tag_keeps_an_inherited_public_property() {
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
"#;
    let text = hover_last(&backend, content, "items;");
    assert!(text.contains("public array $items;"), "{text}");
    assert!(text.contains("The inherited items."), "{text}");
    assert!(!text.contains("virtual"), "{text}");
    assert!(!text.contains("list<string>"), "{text}");
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
fn hover_phpdoc_tag_keeps_a_trait_property() {
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
    assert!(!text.contains("virtual"), "{text}");
    assert!(!text.contains("list<string>"), "{text}");
}
