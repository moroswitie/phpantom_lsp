//! PHPDoc types and texts an override inherits from its parent, its
//! interfaces and its traits (D-03 to D-06, issues 23 and 24). Types are
//! PHPStan 2.3.1's; texts follow PhpStorm 2025.2.2 (D-10).

use crate::common::{assert_var_type, hover_markdown};

const D3_INHERIT: &str =
    include_str!("../conformance/php-symbol-metadata/inheritance/partial-docs.php");

const E1_SPEC_D8: &str = r#"<?php
class P8 {
    /** @var non-empty-string */
    public string $label = 'x';
    /**
     * @var array
     * @phpstan-var list<int>
     * @psalm-var list<string>
     */
    public array $vendor = [];
    public function plain(int $id): void {}
    /** @param positive-int $id */
    public function same(int $id): void {}
    /** @param int $id */
    public function both(int $id): void {}
}
class C8 extends P8 {
    /** @param positive-int $id */
    public function plain(int $id): void { $__v = $id; }
    /** @param positive-int $id */
    public function same(int $id): void { $__v = $id; }
    /** @param int $id */
    public function both(int $id): void { $__v = $id; }
}
$__label = (new P8())->label;
$__vendor = (new P8())->vendor;
"#;

const P7_DESCRIPTIONS: &str =
    include_str!("../conformance/php-symbol-metadata/docblocks/descriptions.php");

/// An override without a docblock, with a summary only, with renamed
/// parameters, or two levels down, takes the parent's parameter and
/// return types.
///
/// Spec: INH-001, INH-005, INH-010.
#[test]
fn d03_undocumented_override_inherits_parent_doc_types() {
    for (var, expected) in [
        ("$__d3", "positive-int"),
        ("$__d4", "non-empty-string"),
        ("$__d5", "positive-int"),
        ("$__d6", "non-empty-string"),
        ("$__d7", "positive-int"),
        ("$__d8", "non-empty-string"),
        ("$__d9", "positive-int"),
        ("$__d11", "non-empty-string"),
        ("$__d14", "non-empty-string"),
    ] {
        assert_var_type(D3_INHERIT, var, expected);
    }
}

/// A child docblock that tags only some parameters still inherits the
/// parent's type for the untagged ones; a property's own `@var` stands.
///
/// Spec: INH-006, PROP-004.
#[test]
fn d04_partial_child_doc_inherits_untagged_params() {
    for (var, expected) in [("$__d2", "non-empty-string"), ("$__d12", "string")] {
        assert_var_type(D3_INHERIT, var, expected);
    }
}

/// With two interfaces documenting the same method, the first one listed
/// wins; a class's own override of a trait method inherits nothing from
/// the trait.
///
/// Spec: INH-007.
#[test]
fn d05_first_listed_interface_wins_and_trait_override_does_not_inherit() {
    for (var, expected) in [
        ("$__d15", "non-empty-string"),
        ("$__d16", "non-empty-string"),
        ("$__d17", "lowercase-string"),
        ("$__d18", "non-empty-string"),
        ("$__d19", "string"),
    ] {
        assert_var_type(D3_INHERIT, var, expected);
    }
}

/// A child's own `@param` narrows an undocumented parent parameter, agrees
/// with an equal parent tag, and an explicit `@param int` stays `int`.
///
/// Spec: INH-002, INH-003, INH-019.
#[test]
fn inh_002_003_019_child_param_docs() {
    for (needle, expected) in [
        (
            "$__v = $id; }\n    /** @param positive-int $id */",
            "positive-int",
        ),
        ("$__v = $id; }\n    /** @param int $id */", "positive-int"),
        ("$__v = $id; }\n}", "int"),
    ] {
        let markdown = hover_markdown(E1_SPEC_D8, needle, 1, 0)
            .unwrap_or_else(|| panic!("expected a hover at {needle:?}"));
        assert!(
            markdown.contains(&format!("$__v = {expected}\n")),
            "{needle:?}: expected `{expected}`, got:\n{markdown}"
        );
    }
}

/// A property's `@var` narrows its native type, and a vendor-prefixed
/// `@phpstan-var` wins over the plain and `@psalm-var` tags.
///
/// Spec: PROP-005, PROP-007.
#[test]
fn prop_005_007_property_doc_types() {
    for (var, expected) in [("$__label", "non-empty-string"), ("$__vendor", "list<int>")] {
        assert_var_type(E1_SPEC_D8, var, expected);
    }
}

/// The hover on a redeclared property without a docblock shows the
/// parent's `@var` type and text.
///
/// Spec: INH-014.
#[test]
fn issue_24_member_hover_shows_inherited_var_type_and_text() {
    let markdown = hover_markdown(P7_DESCRIPTIONS, "$n->label;", 5, 0).expect("expected a hover");
    assert!(
        markdown.contains("**var** `non-empty-string`"),
        "inherited @var type: {markdown}"
    );
    assert!(
        markdown.contains("Parent label text."),
        "inherited @var text: {markdown}"
    );
}

/// An explicit child `@param int` equal to the native type replaces the
/// parent's narrower tag.
///
/// Spec: INH-004.
#[test]
fn issue_23_explicit_param_equal_to_native_wins() {
    assert_var_type(D3_INHERIT, "$__d1", "int");
}

/// An explicit child `@return string` equal to the native type replaces
/// the parent's narrower tag.
///
/// Spec: INH-004.
#[test]
fn issue_23_explicit_return_equal_to_native_wins() {
    assert_var_type(D3_INHERIT, "$__d10", "string");
}

/// A redeclared property without a docblock keeps the parent's `@var`
/// type when it is read.
///
/// Spec: INH-025.
#[test]
#[ignore = "issue 24 / D-06: a redeclared property without a docblock loses the parent's @var type"]
fn issue_24_redeclared_property_inherits_parent_var_type() {
    assert_var_type(D3_INHERIT, "$__d13", "non-empty-string");
}
