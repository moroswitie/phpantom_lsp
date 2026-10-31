//! Modifier display in class and member hover declarations.

use crate::common::{
    create_psr4_workspace, create_test_backend, hover_at, hover_last, hover_text, position_of_last,
};
use phpantom_lsp::Backend;
use tower_lsp_server::gen_lsp_types::{Position, Range};

fn php_declaration(text: &str) -> &str {
    text.split_once("```php\n<?php\n")
        .expect("expected a PHP declaration")
        .1
        .split_once("\n```")
        .expect("expected a closing fence")
        .0
}

fn declaration_at(backend: &Backend, content: &str, needle: &str) -> String {
    php_declaration(&hover_last(backend, content, needle)).to_string()
}

#[test]
fn hover_modifiers_class_type_hints() {
    let backend = create_test_backend();
    for modifiers in [
        "",
        "abstract ",
        "final ",
        "readonly ",
        "abstract readonly ",
        "final readonly ",
    ] {
        let content = format!(
            "<?php\n{modifiers}class Example {{}}\nfunction accepts(Example $value): void {{}}\n"
        );
        let class = backend.parse_php(&content).remove(0);
        assert_eq!(class.is_readonly, modifiers.contains("readonly"));
        assert_eq!(class.is_abstract, modifiers.contains("abstract"));
        assert_eq!(class.is_final, modifiers.contains("final"));
        assert_eq!(
            declaration_at(&backend, &content, "Example $value"),
            format!("{modifiers}class Example")
        );
    }
}

#[test]
fn hover_modifiers_constructor_wrappers() {
    let backend = create_test_backend();
    for modifiers in ["", "final ", "readonly ", "final readonly "] {
        let content = format!(
            "<?php\n{modifiers}class Example {{ public function __construct(int $id) {{}} }}\nnew Example(1);\n"
        );
        assert_eq!(
            declaration_at(&backend, &content, "Example(1)"),
            format!("{modifiers}class Example {{\n    public function __construct(int $id);\n}}")
        );
        let without_constructor = format!("<?php\n{modifiers}class Example {{}}\nnew Example();\n");
        assert_eq!(
            declaration_at(&backend, &without_constructor, "Example()"),
            format!("{modifiers}class Example")
        );
    }
}

#[test]
fn hover_modifiers_member_wrappers() {
    let backend = create_test_backend();
    for modifiers in [
        "",
        "abstract ",
        "final ",
        "readonly ",
        "abstract readonly ",
        "final readonly ",
    ] {
        let content = format!(
            r#"<?php
{modifiers}class Example {{
    public string $label;
    public const ID = 1;
    public function ping(): void {{}}
}}
function inspect(Example $value): void {{
    $value->label;
    $value->ping();
    Example::ID;
}}
"#
        );
        let class = backend.parse_php(&content).remove(0);
        assert!(
            !class.properties[0].is_readonly,
            "implicit readonly stays on the class"
        );
        for (needle, member) in [
            ("label;", "public string $label;"),
            ("ping();", "public function ping(): void;"),
            ("ID;", "public const ID = 1;"),
        ] {
            assert_eq!(
                declaration_at(&backend, &content, needle),
                format!("{modifiers}class Example {{\n    {member}\n}}")
            );
        }
    }
}

#[test]
fn hover_modifiers_explicit_properties() {
    let backend = create_test_backend();
    let content = r#"<?php
class Example {
    public readonly string $ordinary;
    public string $plain;
    public static int $counter = 0;
    /** @readonly */
    public string $documented;
    public $untyped;
    public function __construct(public readonly int $promoted) { $this->ordinary = 'value'; }
}
function inspect(Example $value): void {
    $value->ordinary;
    $value->promoted;
    $value->plain;
    Example::$counter;
    $value->documented;
    $value->untyped;
}
"#;
    let class = backend.parse_php(content).remove(0);
    assert!(class.get_property("ordinary").unwrap().is_readonly);
    assert!(class.get_property("promoted").unwrap().is_readonly);
    assert!(!class.get_property("documented").unwrap().is_readonly);
    assert_ne!(
        class.get_property("documented").unwrap().doc_readonly,
        phpantom_lsp::types::DocReadonly::No
    );
    for (needle, member) in [
        ("ordinary;", "public readonly string $ordinary;"),
        ("promoted;", "public readonly int $promoted;"),
        ("plain;", "public string $plain;"),
        ("$counter;", "public static int $counter;"),
        ("documented;", "public string $documented;"),
        ("untyped;", "public $untyped;"),
    ] {
        assert_eq!(
            declaration_at(&backend, content, needle),
            format!("class Example {{\n    {member}\n}}")
        );
    }
}

#[test]
fn hover_modifiers_final_methods() {
    let backend = create_test_backend();
    let content = "<?php\nclass Example { final public static function sealed(): void {} }\nExample::sealed();\n";
    assert!(backend.parse_php(content)[0].methods[0].is_final);
    assert_eq!(
        declaration_at(&backend, content, "sealed();"),
        "class Example {\n    final public static function sealed(): void;\n}"
    );
}

#[test]
fn hover_modifiers_abstract_methods_and_interface_control() {
    let backend = create_test_backend();
    let content = r#"<?php
abstract class Example { abstract protected function required(): string; }
interface Contract { public function required(): string; }
abstract class Consumer extends Example {
    public function inspect(Contract $value): void {
        $this->required();
        $value->required();
    }
}
"#;
    let classes = backend.parse_php(content);
    assert!(classes[0].methods[0].is_abstract);
    assert!(classes[1].methods[0].is_abstract);
    assert_eq!(
        declaration_at(&backend, content, "required();\n        $value"),
        "abstract class Example {\n    abstract protected function required(): string;\n}"
    );
    assert_eq!(
        declaration_at(&backend, content, "required();"),
        "interface Contract {\n    public function required(): string;\n}"
    );
}

#[test]
fn hover_modifiers_trait_summary_and_imported_members() {
    let backend = create_test_backend();
    let content = r#"<?php
trait Features {
    public readonly string $label;
    public static int $counter = 0;
    public const ID = 1;
    final public static function sealed(): void {}
    abstract public function required(): string;
    public function plain(): void {}
}
abstract class Consumer { use Features; }
function inspect(Consumer $value): void {
    $value->label;
    $value->sealed();
    $value->required();
}
"#;
    assert_eq!(
        declaration_at(&backend, content, "Features;"),
        "trait Features {\n    const ID = 1;\n    public readonly string $label;\n    public static int $counter;\n    final public static function sealed(): void;\n    abstract public function required(): string;\n    public function plain(): void;\n}"
    );
    for (needle, member) in [
        ("label;", "public readonly string $label;"),
        ("sealed();", "final public static function sealed(): void;"),
        (
            "required();",
            "abstract public function required(): string;",
        ),
    ] {
        assert_eq!(
            declaration_at(&backend, content, needle),
            format!("trait Features {{\n    {member}\n}}")
        );
    }
}

#[test]
fn hover_modifiers_inherited_readonly_owner() {
    let backend = create_test_backend();
    let content = r#"<?php
abstract readonly class Ancestor {
    public string $implicit;
    public readonly int $explicit;
    public const ID = 1;
    final public function sealed(): void {}
}
final readonly class Child extends Ancestor {}
function inspect(Child $value): void {
    $value->implicit;
    $value->explicit;
    $value->sealed();
    Child::ID;
}
"#;
    for (needle, member) in [
        ("implicit;", "public string $implicit;"),
        ("explicit;", "public readonly int $explicit;"),
        ("sealed();", "final public function sealed(): void;"),
        ("ID;", "public const ID = 1;"),
    ] {
        assert_eq!(
            declaration_at(&backend, content, needle),
            format!("abstract readonly class Ancestor {{\n    {member}\n}}")
        );
    }
}

#[test]
fn hover_modifiers_namespaces_and_cross_file_imports() {
    let (backend, _dir) = create_psr4_workspace(
        r#"{"autoload":{"psr-4":{"Library\\":"src/"}}}"#,
        &[(
            "src/Example.php",
            "<?php\nnamespace Library;\nfinal readonly class Example { public string $label; }\n",
        )],
    );
    let content = r#"<?php
namespace { class Example { public string $label; } }
namespace App {
    use Library\Example as Imported;
    abstract class Example { public string $label; }
    function inspect(Imported $imported, Example $local, \Example $global): void {
        $imported->label;
        $local->label;
        $global->label;
    }
}
"#;
    for (needle, expected) in [
        (
            "Imported $imported",
            "namespace Library;\nfinal readonly class Example",
        ),
        ("Example $local", "namespace App;\nabstract class Example"),
        ("\\Example $global", "class Example"),
        (
            "$imported->label",
            "namespace Library;\nfinal readonly class Example {\n    public string $label;\n}",
        ),
        (
            "$local->label",
            "namespace App;\nabstract class Example {\n    public string $label;\n}",
        ),
        (
            "$global->label",
            "class Example {\n    public string $label;\n}",
        ),
    ] {
        let token = if needle.starts_with('$') {
            "label"
        } else {
            needle
        };
        let position = position_of_last(content, needle);
        let character = position.character + needle.find(token).unwrap() as u32;
        let hover = hover_at(
            &backend,
            "file:///test.php",
            content,
            position.line,
            character,
        )
        .expect("expected hover");
        assert_eq!(php_declaration(hover_text(&hover)), expected);
    }
}

#[test]
fn hover_modifiers_preserve_documentation_and_types() {
    let backend = create_test_backend();
    let content = r#"<?php
/**
 * A collection of values.
 * @template T
 * @link https://example.com/collection
 */
final class Collection {
    /**
     * Stored values.
     * @var list<T>
     * @deprecated Use items instead.
     */
    public readonly array $values;
    /**
     * Find a value.
     * @return T The selected value.
     * @link https://example.com/find
     */
    final public function find(): mixed {}
}
/** @param Collection<string> $value */
function inspect(Collection $value): void {
    $value->values;
    $value->find();
}
"#;
    let property = hover_last(&backend, content, "values;");
    assert_eq!(
        php_declaration(&property),
        "final class Collection {\n    public readonly array $values;\n}"
    );
    assert!(property.contains("Stored values."), "{property}");
    assert!(property.contains("**var** `list<string>`"), "{property}");
    assert!(
        property.contains("deprecated** Use items instead."),
        "{property}"
    );
    let method = hover_last(&backend, content, "find();");
    assert_eq!(
        php_declaration(&method),
        "final class Collection {\n    final public function find(): mixed;\n}"
    );
    for expected in [
        "Find a value.",
        "**return** `string`",
        "The selected value.",
        "[https://example.com/find](https://example.com/find)",
    ] {
        assert!(method.contains(expected), "{method}");
    }
    let class = hover_last(&backend, content, "Collection $value");
    assert_eq!(php_declaration(&class), "final class Collection");
    for expected in [
        "A collection of values.",
        "**template** `T`",
        "[https://example.com/collection](https://example.com/collection)",
    ] {
        assert!(class.contains(expected), "{class}");
    }
}

#[test]
fn hover_modifiers_other_kinds_and_faults() {
    let backend = create_test_backend();
    let content = r#"<?php
interface Contract { public function run(): void; }
enum Unit { case Ready; }
enum Backed: string { case Ready = 'ready'; }
function inspect(Contract $value, Unit $unit, Backed $backed): void {
    $value->run();
    Unit::Ready;
    Backed::Ready;
    Unknown::missing;
}
class Incomplete {
"#;
    for (needle, expected) in [
        ("Contract $value", "interface Contract"),
        (
            "Unit $unit",
            "enum Unit implements UnitEnum {\n    case Ready;\n}",
        ),
        (
            "Backed $backed",
            "enum Backed implements BackedEnum {\n    case Ready = 'ready';\n}",
        ),
        (
            "run();",
            "interface Contract {\n    public function run(): void;\n}",
        ),
        (
            "Ready;",
            "enum Backed: string {\n    case Ready = 'ready';\n}",
        ),
    ] {
        assert_eq!(declaration_at(&backend, content, needle), expected);
    }
    let missing = position_of_last(content, "missing");
    assert!(
        hover_at(
            &backend,
            "file:///test.php",
            content,
            missing.line,
            missing.character
        )
        .is_none()
    );
}

#[test]
fn hover_modifiers_ranges_and_default_declaration_suppression() {
    let backend = create_test_backend();
    let content = "<?php\nfinal readonly class Example { public string $label; }\nfunction inspect(Example $value): void { /* 🐘 */ $value->label; }\n";
    let position = position_of_last(content, "label;");
    let hover = hover_at(
        &backend,
        "file:///test.php",
        content,
        position.line,
        position.character,
    )
    .expect("expected property hover");
    assert_eq!(
        php_declaration(hover_text(&hover)),
        "final readonly class Example {\n    public string $label;\n}"
    );
    assert_eq!(
        hover.range,
        Some(Range::new(
            position,
            Position::new(position.line, position.character + 5)
        ))
    );
    for needle in ["Example {", "$label;"] {
        let position = position_of_last(content, needle);
        assert!(
            hover_at(
                &backend,
                "file:///test.php",
                content,
                position.line,
                position.character
            )
            .is_none()
        );
    }
}
