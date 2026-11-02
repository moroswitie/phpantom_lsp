//! The types of `static` and `global` locals (D-17, issues 12 to 15 and
//! 25), as PhpStorm 2025.2.2 infers them.

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

const S1_STATIC_LITERAL: &str = r#"<?php
function f(): void {
    static $count = 9;
    $read = $count;
}
"#;

const S2_STATIC_OBJECT: &str = r#"<?php
class Logger {}
function f(): void {
    static $logger = new Logger();
    $read = $logger;
}
"#;

const S3_GLOBAL: &str = r#"<?php
class Database {}
$db = new Database();
function f(): void {
    global $db;
    $read = $db;
}
"#;

const S4_METHOD_STATIC: &str = r#"<?php
class A { public function f(): void { static $n = 3; $read = $n; } }
"#;

const D2_STATIC_AFTER_ASSIGNMENT: &str = r#"<?php
class Logger {}
function work(): void {
    $logger = 1;
    static $logger = new Logger();
    $read = $logger;
}
"#;

const D4_GLOBAL_NEWLINE: &str = r#"<?php
class Database {}
$db = new Database();
function work(): void {
    global
        $db;
    $read = $db;
}
"#;

const E3_NESTED_FUNCTION: &str = r#"<?php
class Logger {}
function outer(): void {
    $cache = 'outer';
    function inner(): void {
        static $cache = new Logger();
        $read = $cache;
    }
}
"#;

const E4_CLOSURE_STATIC: &str = r#"<?php
class Logger {}
$closure = function (): void {
    static $cache = new Logger();
    $read = $cache;
};
"#;

/// A function's `static` local initialised with an object and a `global`
/// declared on one line take the initializer's and the global's types.
#[test]
fn d17_static_and_global_inference_controls() {
    for (name, content, expected) in [
        ("S2", S2_STATIC_OBJECT, "Logger"),
        ("S3", S3_GLOBAL, "Database"),
    ] {
        let markdown = var_hover(content, "$read");
        assert!(
            markdown.contains(&format!("$read = {expected}\n")),
            "{name}: expected `{expected}`, got:\n{markdown}"
        );
    }
}

/// A named function declared inside another resolves its locals in its
/// own scope, not the outer function's.
#[test]
#[ignore = "issue 12 / D-17: a nested named function resolves variables in the outer scope"]
fn issue_12_a_nested_function_uses_its_own_scope() {
    assert_var_type(E3_NESTED_FUNCTION, "$read", "Logger");
}

/// A `static` declaration replaces the type of an earlier assignment.
#[test]
#[ignore = "issue 13 / D-17: a static declaration keeps the type of an earlier assignment"]
fn issue_13_a_static_replaces_an_earlier_assignment() {
    assert_var_type(D2_STATIC_AFTER_ASSIGNMENT, "$read", "Logger");
}

/// A closure's initialised `static` local is typed.
#[test]
#[ignore = "issue 14 / D-17: an initialised static local in a closure is never typed"]
fn issue_14_a_closure_static_is_typed() {
    assert_var_type(E4_CLOSURE_STATIC, "$read", "Logger");
}

/// A `global` statement with a line break after the keyword still types
/// the variable.
#[test]
#[ignore = "issue 15 / D-17: global inference needs the exact spelling `global `"]
fn issue_15_global_with_a_line_break() {
    assert_var_type(D4_GLOBAL_NEWLINE, "$read", "Database");
}

/// A function's `static` local initialised with a literal is widened to
/// the literal's general type.
#[test]
#[ignore = "issue 25 / D-17: a static local keeps its initializer's literal type"]
fn issue_25_a_function_static_literal_is_widened() {
    assert_var_type(S1_STATIC_LITERAL, "$read", "int");
}

/// A method's `static` local initialised with a literal is widened to the
/// literal's general type.
#[test]
#[ignore = "issue 25 / D-17: a static local keeps its initializer's literal type"]
fn issue_25_a_method_static_literal_is_widened() {
    assert_var_type(S4_METHOD_STATIC, "$read", "int");
}
