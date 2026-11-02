//! PHPDoc types and texts an override inherits from its parent, its
//! interfaces and its traits (D-03 to D-06, issues 23 and 24). Types are
//! PHPStan 2.3.1's; texts follow PhpStorm 2025.2.2 (D-10).

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

const D3_INHERIT: &str = r#"<?php
class P {
    /**
     * @param positive-int $id
     * @param non-empty-string $name
     * @return non-empty-string
     */
    public function find(int $id, string $name): string { return 'x'; }
    /** @var non-empty-string */
    public string $label = 'x';
}
class ExplicitEqual extends P {
    /**
     * @param int $id
     * @return string
     */
    public function find(int $id, string $name): string { $__d1 = $id; $__d2 = $name; return ''; }
    /** @var string */
    public string $label = '';
}
class NoDoc extends P {
    public function find(int $id, string $name): string { $__d3 = $id; $__d4 = $name; return 'x'; }
    public string $label = 'x';
}
class DescriptionOnly extends P {
    /** Only a summary, no tags. */
    public function find(int $id, string $name): string { $__d5 = $id; $__d6 = $name; return 'x'; }
}
class Renamed extends P {
    public function find(int $key, string $title): string { $__d7 = $key; $__d8 = $title; return 'x'; }
}
class Grand extends NoDoc {
    public function find(int $id, string $name): string { $__d9 = $id; return 'x'; }
}
$__d10 = (new ExplicitEqual())->find(1, 'a');
$__d11 = (new NoDoc())->find(1, 'a');
$__d12 = (new ExplicitEqual())->label;
$__d13 = (new NoDoc())->label;
$__d14 = (new Grand())->find(1, 'a');
interface I1 { /** @return non-empty-string */ public function name(): string; }
interface I2 { /** @return lowercase-string */ public function name(): string; }
class Impl1 implements I1 { public function name(): string { return 'x'; } }
class Impl12 implements I1, I2 { public function name(): string { return 'x'; } }
class Impl21 implements I2, I1 { public function name(): string { return 'x'; } }
$__d15 = (new Impl1())->name();
$__d16 = (new Impl12())->name();
$__d17 = (new Impl21())->name();
trait T { /** @return non-empty-string */ public function t(): string { return 'x'; } }
class UsesT { use T; }
class OverridesT { use T; public function t(): string { return 'x'; } }
$__d18 = (new UsesT())->t();
$__d19 = (new OverridesT())->t();
"#;

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

const P7_DESCRIPTIONS: &str = r#"<?php

/** @property string $name Parent tag text. */
class TagParent { public function __get(string $n): mixed { return null; } }
/** @property non-empty-string $name */
class TagChildNoText extends TagParent {}
/** @property non-empty-string $name Child tag text. */
class TagChildText extends TagParent {}

class MP {
    /**
     * Parent summary.
     *
     * @param positive-int $id Parent param text.
     * @return non-empty-string Parent return text.
     */
    public function find(int $id): string { return 'x'; }

    /** @var non-empty-string Parent label text. */
    public string $label = 'x';
}
class MExplicitNoText extends MP {
    /** @param int $id */
    public function find(int $id): string { $__e = $id; return ''; }
}
class MNoDoc extends MP {
    public function find(int $id): string { $__n = $id; return 'x'; }
    public string $label = 'x';
}
class MSummaryOnly extends MP {
    /** Child summary only. */
    public function find(int $id): string { $__s = $id; return 'x'; }
}
class MInlineInherit extends MP {
    /** {@inheritDoc} Child extra text. */
    public function find(int $id): string { $__i = $id; return 'x'; }
}
class MTagInherit extends MP {
    /** @inheritDoc */
    public function find(int $id): string { $__t = $id; return 'x'; }
}

/** @property list<string> $items Tag text for items. */
class OwnRealDoc {
    /** @var array Real property text. */
    public array $items = [];
}

function probe(TagChildNoText $a, TagChildText $b, MExplicitNoText $e, MNoDoc $n, MSummaryOnly $s, MInlineInherit $i, MTagInherit $t, OwnRealDoc $o): void {
    $a->name;
    $b->name;
    $e->find(1);
    $n->find(1);
    $s->find(1);
    $i->find(1);
    $t->find(1);
    $n->label;
    $o->items;
    $__ret_e = $e->find(1);
    $__ret_n = $n->find(1);
    $__items = $o->items;
}
"#;

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
#[ignore = "issue 23 / D-06: an explicit child @param equal to the native type is dropped for the parent's tag"]
fn issue_23_explicit_param_equal_to_native_wins() {
    assert_var_type(D3_INHERIT, "$__d1", "int");
}

/// An explicit child `@return string` equal to the native type replaces
/// the parent's narrower tag.
///
/// Spec: INH-004.
#[test]
#[ignore = "issue 23 / D-06: an explicit child @return equal to the native type is dropped for the parent's tag"]
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
