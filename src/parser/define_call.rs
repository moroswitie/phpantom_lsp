//! The one recogniser of `define()` constant declarations.
//!
//! The constant index (`Backend::extract_defines_from_statements`) and the
//! symbol map (`symbol_map/extraction/expressions/calls.rs`) both ask
//! "is this call a `define()` declaration, and of which name?".  They ask it
//! here, so they agree by construction (issues 6 and 8, decision D-14).
//!
//! A `define()` callee is `define` in any case, with or without one
//! leading backslash (`\define(...)` inside a namespace calls the global
//! function).  The first argument must be a string literal with a
//! non-empty value; its offset is the first byte inside the quotes, the
//! way a `const` declaration's points at its identifier.
//!
//! The byte-level prefilter in `classmap_scanner/lexer.rs` stays separate:
//! it reads bytes, not a syntax tree, and accepts a superset of what this
//! module accepts.

use mago_span::HasSpan;
use mago_syntax::cst::*;

use crate::atom::{bytes_to_str, literal_bytes_to_str};

/// A recognised `define('NAME', value)` call.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DefineCall<'a> {
    /// The constant name: the decoded string value, as PHP registers it
    /// (a leading `\` included, see D-13).
    pub name: &'a str,
    /// The byte offset of the first byte inside the name's quotes.
    pub name_offset: u32,
    /// The second argument, the constant's value, when written.
    pub value: Option<&'a Expression<'a>>,
}

/// Whether `function`, the callee of a function call, is PHP's `define`:
/// the name in any case, with or without one leading backslash.
pub(crate) fn is_define_callee(function: &Expression<'_>) -> bool {
    let Expression::Identifier(ident) = function else {
        return false;
    };
    let raw = bytes_to_str(ident.value());
    raw.strip_prefix('\\')
        .unwrap_or(raw)
        .eq_ignore_ascii_case("define")
}

/// `call` as a `define()` declaration, or `None` when it is not one.
pub(crate) fn define_call<'a>(call: &'a FunctionCall<'a>) -> Option<DefineCall<'a>> {
    if !is_define_callee(call.function) {
        return None;
    }
    let mut arguments = call.argument_list.arguments.iter();
    let Expression::Literal(Literal::String(name_literal)) = arguments.next()?.value() else {
        return None;
    };
    let name = name_literal.value.and_then(literal_bytes_to_str)?;
    if name.is_empty() {
        return None;
    }
    Some(DefineCall {
        name,
        name_offset: name_literal.span.start.offset + 1,
        value: arguments.next().map(Argument::value),
    })
}

/// Call `found` with every `define()` declaration that an expression
/// statement's `expression` makes: the expression itself, and the calls
/// inside the operands of `or`, `||`, `and`, `&&`, the branches of a
/// ternary and parentheses, so the guard idioms
/// `defined('X') or define('X', 1)` and
/// `PHP_VERSION_ID >= 80000 ? define('X', 1) : define('X', 2)` count.
pub(crate) fn for_each_define_call<'a>(
    expression: &'a Expression<'a>,
    found: &mut impl FnMut(DefineCall<'a>),
) {
    match expression {
        Expression::Call(Call::Function(call)) => {
            if let Some(define) = define_call(call) {
                found(define);
            }
        }
        Expression::Binary(binary)
            if matches!(
                binary.operator,
                BinaryOperator::And(_)
                    | BinaryOperator::Or(_)
                    | BinaryOperator::LowAnd(_)
                    | BinaryOperator::LowOr(_)
            ) =>
        {
            for_each_define_call(binary.lhs, found);
            for_each_define_call(binary.rhs, found);
        }
        Expression::Conditional(conditional) => {
            if let Some(then) = conditional.then {
                for_each_define_call(then, found);
            }
            for_each_define_call(conditional.r#else, found);
        }
        Expression::Parenthesized(parenthesized) => {
            for_each_define_call(parenthesized.expression, found);
        }
        _ => {}
    }
}

/// The source text of `expression` in `content`.
pub(crate) fn source_text(expression: &Expression<'_>, content: &str) -> Option<String> {
    let span = expression.span();
    content
        .get(span.start.offset as usize..span.end.offset as usize)
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `(name, name_offset, value)` of every `define()` the first
    /// expression statement of `code` declares.
    fn defines(code: &str) -> Vec<(String, u32, Option<String>)> {
        crate::parser::with_parsed_program(code, "define_call_test", |program, content| {
            let expression = program
                .statements
                .iter()
                .find_map(|statement| match statement {
                    Statement::Expression(statement) => Some(statement.expression),
                    _ => None,
                })
                .expect("an expression statement");
            let mut found = Vec::new();
            for_each_define_call(expression, &mut |define| {
                found.push((
                    define.name.to_string(),
                    define.name_offset,
                    define.value.and_then(|value| source_text(value, content)),
                ));
            });
            found
        })
    }

    fn names(code: &str) -> Vec<String> {
        defines(code).into_iter().map(|(name, ..)| name).collect()
    }

    #[test]
    fn a_plain_define() {
        let code = "<?php define('LIMIT', 5);";
        let found = defines(code);
        assert_eq!(
            found,
            vec![(
                "LIMIT".to_string(),
                code.find("LIMIT").unwrap() as u32,
                Some("5".to_string())
            )]
        );
    }

    #[test]
    fn the_callee_in_any_case_with_one_leading_backslash() {
        assert_eq!(names("<?php DEFINE('A', 1);"), ["A"]);
        assert_eq!(names("<?php \\define('A', 1);"), ["A"]);
        assert_eq!(names("<?php \\Define('A', 1);"), ["A"]);
        assert!(names("<?php App\\define('A', 1);").is_empty());
        assert!(names("<?php defined('A');").is_empty());
    }

    #[test]
    fn inside_boolean_operands_ternaries_and_parentheses() {
        assert_eq!(names("<?php defined('A') or define('A', 1);"), ["A"]);
        assert_eq!(names("<?php defined('A') || define('A', 1);"), ["A"]);
        assert_eq!(names("<?php $ok and define('A', 1);"), ["A"]);
        assert_eq!(names("<?php $ok && (define('A', 1));"), ["A"]);
        assert_eq!(
            names("<?php $new ? define('A', 1) : define('A', 2);"),
            ["A", "A"]
        );
        assert!(names("<?php $x = define('A', 1);").is_empty());
    }

    #[test]
    fn the_name_must_be_a_non_empty_string_literal() {
        assert!(names("<?php define($name, 1);").is_empty());
        assert!(names("<?php define('', 1);").is_empty());
        assert_eq!(defines("<?php define('A');"), [("A".to_string(), 14, None)]);
    }
}
