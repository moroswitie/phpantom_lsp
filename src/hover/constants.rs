//! Global constant hover.
//!
//! A usage's value comes from the constant lookup chain in
//! `resolution/constants.rs`; a declaration's from the index entry
//! that declaration recorded.

use tower_lsp_server::gen_lsp_types::Hover;

use crate::Backend;

use super::HoverSite;
use super::formatting::make_hover;
use super::function::declared_global_name;

impl Backend {
    /// Hover for a global constant reference: its value when known.
    pub(super) fn hover_constant_reference(
        &self,
        site: &HoverSite,
        symbol_start: u32,
        name: &str,
    ) -> Option<Hover> {
        let ctx = site.ctx;
        // The name is resolved against the file the same way the
        // type engine resolves it, so a namespaced constant is
        // found through whichever spelling the reference used.
        let lookup = self.resolve_constant_name_at(
            name,
            ctx.resolved_names.as_deref(),
            symbol_start,
            &ctx.use_map,
            &ctx.symbol_imports.constants,
            &ctx.namespace,
        );

        constant_hover(name, lookup)
    }

    /// Hover for a top-level constant at its declaration site: the
    /// value this file's declaration at this very position gives it.
    ///
    /// No namespace-then-global search, stub or built-in can answer
    /// for the spelling, and neither can another declaration of the
    /// same name (in another file, or in another branch of this one);
    /// when the index holds another one, this declaration answers
    /// nothing. Nor does it when a stub path declares the name in
    /// another file: a usage resolves to the stub path's declaration
    /// first, and loads it into the index only then, so the index alone
    /// can still hold this one.
    ///
    /// A `const` name declares the constant of its namespace block
    /// plus its stored name. A `define()` name is already the full
    /// name it declares, wherever the call sits; the index keeps the
    /// leading `\` of a `define('\\NAME')` that the symbol map strips,
    /// so both spellings of that one name are tried.
    pub(super) fn hover_constant_declaration(
        &self,
        site: &HoverSite,
        symbol_start: u32,
        name: &str,
    ) -> Option<Hover> {
        let define_start = define_name_start(site.content, symbol_start);
        let declared_at = define_start.unwrap_or(symbol_start);
        let (declared_name, value) = {
            let defines = self.symbols.global_defines.read();
            let declared_here = |key: &str| {
                defines
                    .get_key_value(key)
                    .filter(|(_, define)| {
                        define.file_uri == site.uri && define.name_offset == declared_at
                    })
                    .map(|(key, define)| (key.clone(), define.value.clone()))
            };
            if define_start.is_some() {
                declared_here(name).or_else(|| declared_here(&format!("\\{name}")))
            } else {
                let namespace = site.ctx.namespace_at(site.offset);
                declared_here(&declared_global_name(namespace.as_deref(), name))
            }
        }?;
        if self
            .external_stub_constant_uri(&declared_name)
            .is_some_and(|stub_uri| stub_uri != site.uri)
        {
            return None;
        }
        constant_hover(name, Some(value))
    }
}

/// Render a constant lookup as a hover, or `None` when the constant
/// was not found at all.
///
/// `lookup` is `Some(Some(val))` when the constant exists with a
/// known value and `Some(None)` when it exists but the value is
/// unknown.
fn constant_hover(name: &str, lookup: Option<Option<String>>) -> Option<Hover> {
    match lookup {
        Some(Some(val)) => Some(make_hover(format!(
            "```php\n<?php\nconst {} = {};\n```",
            name, val
        ))),
        Some(None) => Some(make_hover(format!("```php\n<?php\nconst {};\n```", name))),
        None => None,
    }
}

/// Where the string contents start when the constant span at
/// `symbol_start` is the name argument of a `define()` call (the
/// offset the index records a `define()` at), or `None` for a `const`
/// name.
///
/// Recognised by source text: after skipping back over `\` bytes (an
/// escaped leading backslash the extractor trimmed from the span) the
/// text before the span ends with the string literal's opening quote.
/// A `const` name never follows a quote.
fn define_name_start(content: &str, symbol_start: u32) -> Option<u32> {
    let before = content.get(..symbol_start as usize)?.trim_end_matches('\\');
    (before.ends_with('\'') || before.ends_with('"')).then_some(before.len() as u32)
}

/// Extract the value of a constant from PHP source text.
///
/// Scans for patterns like:
/// - `define('NAME', value)` or `define("NAME", value)`
/// - `const NAME = value;`
///
/// Returns `Some(value_string)` when found, `None` when the constant
/// definition could not be located or the value could not be extracted.
///
/// **Note:** Production code should use `update_ast` to parse constants
/// through the AST pipeline (which populates `global_defines`).  This
/// function exists only for unit tests.
#[cfg(test)]
pub(super) fn extract_constant_value_from_source(name: &str, source: &str) -> Option<String> {
    // Try `define('NAME', value)` pattern.
    for quote in &["'", "\""] {
        let needle = format!("define({quote}{name}{quote}");
        if let Some(pos) = source.find(&needle) {
            // Extract only the second argument.  Stop at the first
            // unquoted comma (third argument) or closing paren,
            // whichever comes first.
            let after = &source[pos + needle.len()..];
            if let Some(comma) = after.find(',') {
                let value_start = &after[comma + 1..];
                let trimmed = value_start.trim_start();
                // Find where the second argument ends: either an
                // unquoted comma (start of optional third arg) or
                // the closing paren.
                let end =
                    find_unquoted_comma(trimmed).or_else(|| find_balanced_close_paren(trimmed));
                if let Some(end) = end {
                    let val = trimmed[..end].trim();
                    if !val.is_empty() {
                        // Empty string literals are placeholders for
                        // runtime-defined values — show the type instead.
                        if val == "''" || val == "\"\"" {
                            return Some("string".to_string());
                        }
                        return Some(val.to_string());
                    }
                }
            }
        }
    }

    // Try `const NAME = value;` pattern.
    let const_needle = format!("const {name}");
    for (i, _) in source.match_indices(&const_needle) {
        let after = &source[i + const_needle.len()..];
        let trimmed = after.trim_start();
        if let Some(rest) = trimmed.strip_prefix('=') {
            let value_part = rest.trim_start();
            if let Some(semi) = value_part.find(';') {
                let val = value_part[..semi].trim();
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }

    None
}

/// Find the position of the first unquoted comma in `s`.
///
/// Skips over single- and double-quoted string literals so that
/// commas inside string values are not mistaken for argument
/// separators.
#[cfg(test)]
fn find_unquoted_comma(s: &str) -> Option<usize> {
    let mut in_single = false;
    let mut in_double = false;
    let mut prev = b'\0';

    for (i, &b) in s.as_bytes().iter().enumerate() {
        match b {
            b'\'' if !in_double && prev != b'\\' => in_single = !in_single,
            b'"' if !in_single && prev != b'\\' => in_double = !in_double,
            b',' if !in_single && !in_double => return Some(i),
            _ => {}
        }
        prev = b;
    }
    None
}

/// Find the position of the closing `)` that matches an implicit
/// opening paren, handling one level of nesting and string literals.
#[cfg(test)]
fn find_balanced_close_paren(s: &str) -> Option<usize> {
    let mut depth = 0u32;
    let mut in_single = false;
    let mut in_double = false;
    let mut prev = b'\0';

    for (i, &b) in s.as_bytes().iter().enumerate() {
        match b {
            b'\'' if !in_double && prev != b'\\' => in_single = !in_single,
            b'"' if !in_single && prev != b'\\' => in_double = !in_double,
            b'(' if !in_single && !in_double => depth += 1,
            b')' if !in_single && !in_double => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
            }
            _ => {}
        }
        prev = b;
    }
    None
}
