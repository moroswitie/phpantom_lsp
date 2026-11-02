//! PHPDoc type precedence on one declaration: vendor-prefixed tags against
//! the plain tag (D-01) and a docblock type against the native type (D-02).
//! Expected values are PHPStan 2.3.1's.

use crate::common::{create_test_backend, hover_at, hover_text, offset_position};

/// Hover `delta` characters into the `nth` (0-based) occurrence of
/// `needle` in `content`, parsed as `file:///test.php`.
fn hover_markdown(content: &str, needle: &str, delta: u32, nth: usize) -> Option<String> {
    let (offset, _) = content
        .match_indices(needle)
        .nth(nth)
        .unwrap_or_else(|| panic!("occurrence {nth} of {needle:?} not found"));
    let position = offset_position(content, offset + delta as usize);
    let backend = create_test_backend();
    hover_at(
        &backend,
        "file:///test.php",
        content,
        position.line,
        position.character,
    )
    .map(|hover| hover_text(&hover).to_string())
}

/// Shorthand: hover on `"$var ="` (delta 1) and return the markdown.
fn var_hover(content: &str, var: &str) -> String {
    hover_markdown(content, &format!("{var} ="), 1, 0)
        .unwrap_or_else(|| panic!("expected a hover on {var}"))
}

/// Assert the variable hover of `var` renders `"{var} = {expected}"`.
fn assert_var_type(content: &str, var: &str, expected: &str) {
    let markdown = var_hover(content, var);
    assert!(
        markdown.contains(&format!("{var} = {expected}\n")),
        "{var}: expected `{expected}`, got:\n{markdown}"
    );
}

const D1_VENDOR: &str = r#"<?php
class D1 {
    /**
     * @param array $a
     * @phpstan-param list<int> $a
     * @psalm-param list<string> $a
     */
    public function allThree(array $a): void { $__d1 = $a; }
    /**
     * @param array $b
     * @psalm-param list<string> $b
     */
    public function plainAndPsalm(array $b): void { $__d2 = $b; }
    /**
     * @param list<int> $c
     * @phpstan-param array $c
     */
    public function vendorWider(array $c): void { $__d3 = $c; }
    /**
     * @return array
     * @phpstan-return list<int>
     */
    public function ret(): array { return []; }
}
$__d4 = (new D1())->ret();
"#;

const D2_NATIVE: &str = r#"<?php
/** @param string $x */ function incompatible(int $x): void { $__d1 = $x; }
/** @param positive-int $x */ function narrowing(int $x): void { $__d2 = $x; }
/** @param int|string|float $x */ function wider(int|string $x): void { $__d3 = $x; }
/** @param int $x */ function equal(int $x): void { $__d4 = $x; }
/** @return list<string> */ function incompatibleReturn(): int { return 1; }
$__d5 = incompatibleReturn();
class Pd { /** @var string */ public int $p = 1; }
$__d6 = (new Pd())->p;
"#;

/// `@phpstan-*` beats `@psalm-*`, which beats the plain tag, even when the
/// winning tag is wider than a losing one.
///
/// Spec: PAR-002, METH-004.
#[test]
fn d01_vendor_precedence_by_prefix() {
    for (var, expected) in [
        ("$__d1", "list<int>"),
        ("$__d2", "list<string>"),
        ("$__d3", "array"),
        ("$__d4", "list<int>"),
    ] {
        assert_var_type(D1_VENDOR, var, expected);
    }
}

/// A docblock type applies only where it is a subtype of the native type;
/// otherwise the native type stands.
///
/// Spec: PAR-004.
#[test]
fn d02_doc_type_applies_only_as_a_subtype_of_native() {
    for (var, expected) in [
        ("$__d1", "int"),
        ("$__d2", "positive-int"),
        ("$__d3", "int|string"),
        ("$__d4", "int"),
        ("$__d5", "int"),
        ("$__d6", "int"),
    ] {
        assert_var_type(D2_NATIVE, var, expected);
    }
}
