//! Which docblock belongs to a declaration next to attributes (issue 17),
//! and the texts an override inherits (D-10, D-11, issues 23 and 26).
//! Association follows the PHP 8.5 runtime's reflection; texts follow
//! PhpStorm 2025.2.2; types are PHPStan 2.3.1's.

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

/// The member hover at `needle` (first occurrence, `delta` in), which
/// must contain every string of `present` and none of `absent`.
fn assert_member_hover(content: &str, needle: &str, delta: u32, present: &[&str], absent: &[&str]) {
    let markdown = hover_markdown(content, needle, delta, 0)
        .unwrap_or_else(|| panic!("expected a hover at {needle:?}"));
    for text in present {
        assert!(
            markdown.contains(text),
            "{needle:?}: expected {text:?} in:\n{markdown}"
        );
    }
    for text in absent {
        assert!(
            !markdown.contains(text),
            "{needle:?}: expected no {text:?} in:\n{markdown}"
        );
    }
}

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

const B1_PROMOTED_AFTER_ATTRIBUTE: &str = r#"<?php
class Vault {
    public function __construct(
        #[\SensitiveParameter]
        /** @var non-empty-string Own text. */
        public string $secret,
        /** @var positive-int Before text. */
        #[\SensitiveParameter]
        public int $pin,
    ) {}
}
$v = new Vault('x', 1);
echo $v->secret;
echo $v->pin;
"#;

const B2_ATTRIBUTE_THEN_DOCBLOCK: &str = r#"<?php
class Box {
    #[\Deprecated]
    /** @var list<string> Own items text. */
    public array $items = [];
    #[\Deprecated]
    /** @return list<string> Own names text. */
    public function names(): array { return []; }
    /** @var list<string> Control text. */
    #[\Deprecated]
    public array $control = [];
}
$b = new Box();
echo $b->items;
$n = $b->names();
echo $b->control;
"#;

const E2_ATTR_INTERLEAVED_STACKED: &str = r#"<?php
#[Attribute] class A {} #[Attribute] class B {} #[Attribute] class C {}
class Box {
    /** First. */
    #[A]
    /** Second. */
    public array $interleaved = [];
    #[A, B]
    #[C]
    /** Stacked. */
    public array $stacked = [];
}
$b = new Box();
echo $b->interleaved;
echo $b->stacked;
"#;

/// An override without a docblock shows the parent's summary and its
/// `@param` and `@return` types and texts.
#[test]
fn d10_undocumented_override_inherits_summary_and_param_text() {
    assert_member_hover(
        P7_DESCRIPTIONS,
        "    $n->find(1);",
        9,
        &[
            "Parent summary.",
            "**$id** `positive-int`",
            "Parent param text.",
            "**return** `non-empty-string`",
            "Parent return text.",
        ],
        &[],
    );
}

/// An override whose docblock has only a summary keeps that summary and
/// inherits the parent's tags.
#[test]
fn d10_summary_only_child_keeps_its_summary_and_inherits_tags() {
    assert_member_hover(
        P7_DESCRIPTIONS,
        "$s->find(1);",
        5,
        &[
            "Child summary only.",
            "**$id** `positive-int`",
            "Parent param text.",
        ],
        &["Parent summary."],
    );
}

/// A docblock of only `@inheritDoc` inherits the parent's summary, tags
/// and types.
#[test]
fn d11_inheritdoc_tag_inherits_everything() {
    assert_member_hover(
        P7_DESCRIPTIONS,
        "$t->find(1);",
        5,
        &[
            "Parent summary.",
            "**$id** `positive-int`",
            "Parent param text.",
        ],
        &[],
    );
    for var in ["$__t", "$__i"] {
        assert_var_type(P7_DESCRIPTIONS, var, "positive-int");
    }
}

/// A docblock before the attributes of a property belongs to it; so does
/// one before the attribute of a promoted constructor parameter.
///
/// Spec: ATTR-001, PROP-012, PAR-013.
#[test]
fn attr_001_docblock_before_attribute() {
    assert_member_hover(
        B2_ATTRIBUTE_THEN_DOCBLOCK,
        "->control",
        2,
        &["Control text.", "**var** `list<string>`"],
        &[],
    );
    assert_member_hover(
        B1_PROMOTED_AFTER_ATTRIBUTE,
        "->pin",
        2,
        &["**var** `positive-int`"],
        &[],
    );
}

/// Inline `{@inheritDoc}` is replaced by the parent's summary.
#[test]
#[ignore = "issue 26 / D-11: inline {@inheritDoc} is shown literally instead of the parent summary"]
fn issue_26_inline_inheritdoc_expands_the_parent_summary() {
    assert_member_hover(
        P7_DESCRIPTIONS,
        "$i->find(1);",
        5,
        &["Parent summary. Child extra text."],
        &["{@inheritDoc}"],
    );
}

/// An explicit `@param int $id` without text replaces the parent's tag
/// and its text; the untouched `@return` is still inherited.
///
/// Spec: INH-004.
#[test]
fn issue_23_explicit_param_tag_without_text_hides_the_parent_text() {
    assert_member_hover(
        P7_DESCRIPTIONS,
        "    $e->find(1);",
        9,
        &[
            "Parent summary.",
            "**$id** `int`",
            "**return** `non-empty-string`",
            "Parent return text.",
        ],
        &["Parent param text."],
    );
}

/// An explicit `@param int $id` types the parameter in the child's body.
///
/// Spec: INH-004.
#[test]
fn issue_23_explicit_param_tag_types_the_child_body() {
    assert_var_type(P7_DESCRIPTIONS, "$__e", "int");
}

/// A docblock between an attribute and a promoted constructor parameter
/// belongs to the parameter.
///
/// Spec: ATTR-002, PAR-012, COMB-002.
#[test]
fn issue_17_promoted_parameter_docblock_after_attribute() {
    assert_member_hover(
        B1_PROMOTED_AFTER_ATTRIBUTE,
        "->secret",
        2,
        &["**var** `non-empty-string`", "Own text."],
        &[],
    );
}

/// A docblock between an attribute and a property belongs to the property.
///
/// Spec: ATTR-002, PROP-012.
#[test]
fn issue_17_property_docblock_after_attribute() {
    assert_member_hover(
        B2_ATTRIBUTE_THEN_DOCBLOCK,
        "->items",
        2,
        &["**var** `list<string>`", "Own items text."],
        &[],
    );
}

/// A docblock between an attribute and a method belongs to the method.
///
/// Spec: ATTR-002.
#[test]
fn issue_17_method_docblock_after_attribute() {
    assert_member_hover(
        B2_ATTRIBUTE_THEN_DOCBLOCK,
        "->names",
        2,
        &["Own names text."],
        &[],
    );
    assert_var_type(B2_ATTRIBUTE_THEN_DOCBLOCK, "$n", "list<string>");
}

/// Of a docblock before and one after an attribute, the one after (the
/// last before the declaration) belongs to the property.
///
/// Spec: ATTR-003.
#[test]
fn attr_003_the_docblock_after_the_attribute_wins() {
    assert_member_hover(
        E2_ATTR_INTERLEAVED_STACKED,
        "->interleaved",
        2,
        &["Second."],
        &["First."],
    );
}

/// A docblock after several attribute groups belongs to the property.
///
/// Spec: ATTR-004.
#[test]
fn attr_004_docblock_after_stacked_attribute_groups() {
    assert_member_hover(
        E2_ATTR_INTERLEAVED_STACKED,
        "->stacked",
        2,
        &["Stacked."],
        &[],
    );
}
