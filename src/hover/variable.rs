//! Variable hover.
//!
//! Resolves the type of a variable at the cursor through the shared
//! variable-resolution pipeline and renders it, preserving generic
//! parameters and scalar types.  Union types are shown as separate code
//! blocks divided by a horizontal rule.

use std::sync::Arc;

use tower_lsp_server::gen_lsp_types::Hover;

use crate::Backend;
use crate::class_lookup::{Fqn, ResolvedClassLoader, find_class_at_offset};
use crate::php_type::PhpType;
use crate::symbol_map::{SymbolSpan, VarDefKind};
use crate::type_engine::resolver::FileCtx;
use crate::type_engine::variable::resolution::VarQuery;
use crate::types::{ClassInfo, FileContext, FunctionInfo, PropertyInfo};

use super::HoverSite;
use super::formatting::{make_hover, namespace_line};

impl Backend {
    /// Hover for a `$variable` the symbol map found, except at a
    /// definition whose type is already on screen. With hover on
    /// declarations on, a property declaration shows its property.
    pub(super) fn hover_variable_symbol(
        &self,
        site: &HoverSite,
        symbol: &SymbolSpan,
        name: &str,
    ) -> Option<Hover> {
        let HoverSite {
            uri,
            content,
            offset: cursor_offset,
            ctx,
            ..
        } = *site;
        // At a variable definition site, `variable_definition_hover`
        // decides, together with the promoted-parameter check below,
        // whether a hover is shown at all, and which one.
        let definition = self.symbol_map_for(uri).and_then(|map| {
            map.var_def_at(name, cursor_offset)
                .map(|def| (def.kind.clone(), def.is_promoted))
        });
        if let Some((def_kind, is_promoted)) = definition {
            let hover_on_declarations = self.hover_on_declarations_enabled();
            match variable_definition_hover(&def_kind, is_promoted, hover_on_declarations) {
                VariableDefinitionHover::Suppress => return None,
                VariableDefinitionHover::ShowProperty => {
                    let (owner, property) = property_declared_at(&ctx.classes, symbol.start, name)?;
                    return self.hover_property_declaration(site, owner, property);
                }
                // With the switch off, a constructor-promoted parameter
                // declares a property, not a local variable, so it
                // follows the same "already visible on screen" rule as
                // any other property declaration.
                VariableDefinitionHover::ShowVariable
                    if !hover_on_declarations
                        && def_kind == VarDefKind::Parameter
                        && self.is_promoted_property_param(uri, symbol.start) =>
                {
                    return None;
                }
                VariableDefinitionHover::ShowVariable => {}
            }
        }
        self.hover_variable(name, uri, content, cursor_offset, site.current_class(), ctx)
    }

    /// Produce hover information for a variable.
    pub(super) fn hover_variable(
        &self,
        name: &str,
        uri: &str,
        content: &str,
        cursor_offset: u32,
        current_class: Option<&ClassInfo>,
        ctx: &FileContext,
    ) -> Option<Hover> {
        let var_name = format!("${}", name);

        // When the cursor is on the `$` of an assignment like
        // `$x = new Foo()`, the cursor offset equals the assignment
        // statement's start offset.  The variable resolution pipeline
        // skips statements whose start is at or after the cursor
        // (`stmt.start >= cursor_offset`), so the assignment is
        // excluded.  Nudge the offset by one byte so the statement's
        // start is strictly less than the cursor, allowing the
        // assignment to be included.  We only do this for assignments
        // (not parameters, foreach, etc.) where `effective_from`
        // differs from the definition offset.
        let mut cursor_offset = cursor_offset;
        if self
            .lookup_var_def_effective_from(uri, name, cursor_offset)
            .is_some()
        {
            let offset = cursor_offset as usize;
            if let Some(ch) = content.get(offset..).and_then(|s| s.chars().next()) {
                cursor_offset += ch.len_utf8() as u32;
            }
        }

        // $this resolves to the enclosing class, but not inside static methods.
        if name == "this" {
            let in_static = self
                .symbol_map_for(uri)
                .is_some_and(|map| map.is_in_static_method(cursor_offset));
            if !in_static && let Some(cc) = current_class {
                let ns_line = namespace_line(cc.file_namespace.as_deref());
                return Some(make_hover(format!(
                    "```php\n<?php\n{}$this = {}\n```",
                    ns_line, cc.name
                )));
            }
            return Some(make_hover("```php\n<?php\n$this\n```".to_string()));
        }

        let class_loader = self.class_loader(ctx);

        let resolved_class_loader = self.resolved_class_loader(ctx);
        let function_loader = self.function_loader(ctx);
        let constant_loader = self.constant_loader(ctx);
        let config_resolver = |key: &str| self.resolve_config_type(key);
        let trans_resolver = |key: &str| self.resolve_trans_type(key);
        let loaders = crate::type_engine::resolver::Loaders {
            function_loader: Some(&function_loader as &dyn Fn(&str, u32) -> Option<FunctionInfo>),
            constant_loader: Some(&constant_loader),
            config_resolver: Some(&config_resolver),
            trans_resolver: Some(&trans_resolver),
        };

        // Try the type-string path first.  This preserves generic
        // parameters (e.g. `Generator<int, Pencil>`) and scalar types
        // (e.g. `int`) that the ClassInfo-based path would lose.
        if let Some(resolved_type) =
            crate::type_engine::variable::resolution::resolve_variable_php_type(VarQuery {
                loaders,
                ..VarQuery::new(
                    &var_name,
                    FileCtx::new(content, &ctx.classes, &resolved_class_loader, &class_loader)
                        .with_imports(Some(&ctx.use_map))
                        .with_backend(Some(self)),
                    current_class,
                    cursor_offset,
                )
            })
        {
            // When the type is a template parameter, show its variance
            // and bound (e.g. "**template-covariant** `TNode` of `AstNode`")
            // above the code block so the user sees the constraint.
            let template_line =
                self.find_template_info_for_type(&resolved_type, uri, cursor_offset);

            let hover_body = build_variable_hover_body(
                &var_name,
                &resolved_type,
                &resolved_class_loader,
                template_line.as_deref(),
            );
            return Some(make_hover(hover_body));
        }

        // `resolve_variable_php_type` already calls `resolve_variable_types`
        // internally, so if it returned `None` the variable is truly
        // unresolved.  Show a plain variable hover.
        Some(make_hover(format!("```php\n<?php\n{}\n```", var_name)))
    }
}

/// What hover does at a variable definition site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VariableDefinitionHover {
    /// Show the ordinary variable hover.
    ShowVariable,
    /// Show the hover of the property declared here, or nothing when
    /// the file's classes do not record it; never a variable hover.
    ShowProperty,
    /// Show nothing: the declaration is already on screen.
    Suppress,
}

/// The hover policy for one kind of variable definition site.
///
/// Every `VarDefKind` is named, so a kind is never covered by accident
/// through a catch-all. The caller passes whether the definition is a
/// promoted constructor parameter, and the declaration-hover switch.
///
/// With the switch off, the `Parameter` entry is not the whole policy
/// for parameters: the caller also suppresses a promoted parameter,
/// which declares a property rather than a local variable.
fn variable_definition_hover(
    kind: &VarDefKind,
    is_promoted: bool,
    hover_on_declarations: bool,
) -> VariableDefinitionHover {
    match kind {
        VarDefKind::Assignment => VariableDefinitionHover::ShowVariable,
        VarDefKind::CompoundAssignment => VariableDefinitionHover::ShowVariable,
        VarDefKind::Parameter if hover_on_declarations => parameter_declaration_route(is_promoted),
        VarDefKind::Parameter => VariableDefinitionHover::ShowVariable,
        VarDefKind::Property if hover_on_declarations => VariableDefinitionHover::ShowProperty,
        VarDefKind::Property => VariableDefinitionHover::Suppress,
        VarDefKind::Foreach => VariableDefinitionHover::ShowVariable,
        VarDefKind::Catch => VariableDefinitionHover::ShowVariable,
        VarDefKind::StaticDecl => VariableDefinitionHover::Suppress,
        VarDefKind::GlobalDecl => VariableDefinitionHover::Suppress,
        VarDefKind::ArrayDestructuring => VariableDefinitionHover::ShowVariable,
        VarDefKind::ListDestructuring => VariableDefinitionHover::ShowVariable,
        VarDefKind::ClosureCapture => VariableDefinitionHover::Suppress,
        VarDefKind::DocblockParam => VariableDefinitionHover::Suppress,
        VarDefKind::DocblockVar => VariableDefinitionHover::Suppress,
        VarDefKind::Unset => VariableDefinitionHover::Suppress,
    }
}

/// What hover shows at a parameter when hover on declarations is on: a
/// promoted parameter declares a property, any other a local variable.
///
/// Promotion is read from the parameter's own syntax, never from the
/// file's classes, so missing class information can neither turn a
/// promoted property into a variable hover nor hide an ordinary
/// parameter's.
fn parameter_declaration_route(is_promoted: bool) -> VariableDefinitionHover {
    if is_promoted {
        VariableDefinitionHover::ShowProperty
    } else {
        VariableDefinitionHover::ShowVariable
    }
}

/// The class and the real property that the `$name` token at `offset`
/// declares: an ordinary property, or a constructor parameter that
/// promotes one. `None` when `classes`, the file's class information,
/// does not record it.
///
/// The name is checked as well as the offset: class information that
/// lags behind the buffer can hold another property at the same offset,
/// as after a rename to a name of the same length.
fn property_declared_at<'a>(
    classes: &'a [Arc<ClassInfo>],
    offset: u32,
    name: &str,
) -> Option<(&'a ClassInfo, &'a PropertyInfo)> {
    let owner = find_class_at_offset(classes, offset)?;
    let property = owner
        .properties
        .iter()
        .find(|property| property.name_offset == offset && property.name == name)?;
    Some((owner, property))
}

/// Build the hover body for a variable, rendering union types as
/// separate code blocks separated by a horizontal rule (`---`).
///
/// For a single type (or scalar/generic) this produces one code block
/// showing e.g. `$user = User`.
///
/// For a union like `Lamp|Faucet` it produces two code blocks
/// (`$ambiguous = Lamp` and `$ambiguous = Faucet`) joined by a
/// markdown horizontal rule so the editor renders a visible divider.
pub(super) fn build_variable_hover_body(
    var_name: &str,
    ty: &PhpType,
    class_loader: &ResolvedClassLoader<'_>,
    template_line: Option<&str>,
) -> String {
    // A template bounded by a union is still the one template.
    let members = match ty.as_template_param() {
        Some(_) => vec![ty],
        None => ty.union_members(),
    };

    // Count how many members are non-trivial class types (not scalars,
    // not `null`, not `void`, etc.).  Only render separate blocks when
    // there are 2+ class-like types; a simple `Foo|null` should stay
    // in one block, and so should the cases of one enum.
    let mut class_likes: Vec<PhpType> = members
        .iter()
        .filter(|m| !m.is_scalar())
        .map(|m| m.enum_cases_as_enums())
        .collect();
    class_likes.dedup();
    let class_like_count = class_likes.len();

    // When there is only one component, or only one class-like type
    // (the rest being scalars / null), render a single code block.
    if members.len() <= 1 || class_like_count < 2 {
        let short_type = ty.shorten().to_string();
        let ns = resolve_type_namespace_structured(ty, class_loader);
        let ns_line = namespace_line(ns.as_deref());
        let code_block = format!(
            "```php\n<?php\n{}{} = {}\n```",
            ns_line, var_name, short_type
        );
        return if let Some(tpl) = template_line {
            format!("{}\n\n{}", tpl, code_block)
        } else {
            code_block
        };
    }

    // Multiple union branches — render each as its own code block
    // separated by a markdown horizontal rule.
    let mut blocks: Vec<String> = Vec::with_capacity(members.len());
    for member in &members {
        let short = member.shorten().to_string();
        let ns = resolve_type_namespace_structured(member, class_loader);
        let ns_line = namespace_line(ns.as_deref());
        blocks.push(format!(
            "```php\n<?php\n{}{} = {}\n```",
            ns_line, var_name, short
        ));
    }

    let body = blocks.join("\n\n---\n\n");
    if let Some(tpl) = template_line {
        format!("{}\n\n{}", tpl, body)
    } else {
        body
    }
}

/// Extract the namespace for a structured `PhpType` by looking up its
/// base class name via the class loader, or by parsing the namespace
/// from the FQN string itself.
fn resolve_type_namespace_structured(
    ty: &PhpType,
    class_loader: &ResolvedClassLoader<'_>,
) -> Option<String> {
    // A template's name is not in any namespace, whatever its bound is.
    if ty.as_template_param().is_some() {
        return None;
    }
    let base = ty.base_name()?;

    if let Some(cls) = class_loader(Fqn::new(base)) {
        return cls
            .file_namespace
            .as_ref()
            .filter(|ns| !ns.is_empty() && !ns.starts_with("___"))
            .map(|ns| ns.to_string());
    }

    // Fallback: parse the namespace from the FQN string itself.
    // E.g. `App\Models\User` → `App\Models`.
    if let Some(pos) = base.rfind('\\') {
        let ns = &base[..pos];
        if !ns.is_empty() {
            return Some(ns.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_definition_hover_names_every_kind_with_its_current_policy() {
        use VariableDefinitionHover::{ShowProperty, ShowVariable, Suppress};

        // The policy with the switch off, and how the switch changes it.
        let policies = [
            (VarDefKind::Assignment, ShowVariable),
            (VarDefKind::CompoundAssignment, ShowVariable),
            (VarDefKind::Parameter, ShowVariable),
            (VarDefKind::Property, Suppress),
            (VarDefKind::Foreach, ShowVariable),
            (VarDefKind::Catch, ShowVariable),
            (VarDefKind::StaticDecl, Suppress),
            (VarDefKind::GlobalDecl, Suppress),
            (VarDefKind::ArrayDestructuring, ShowVariable),
            (VarDefKind::ListDestructuring, ShowVariable),
            (VarDefKind::ClosureCapture, Suppress),
            (VarDefKind::DocblockParam, Suppress),
            (VarDefKind::DocblockVar, Suppress),
            (VarDefKind::Unset, Suppress),
        ];
        let switched_on = |kind: &VarDefKind, is_promoted: bool, off| match kind {
            VarDefKind::Parameter if is_promoted => ShowProperty,
            VarDefKind::Property => ShowProperty,
            _ => off,
        };
        for is_promoted in [false, true] {
            for (kind, off) in &policies {
                assert_eq!(
                    variable_definition_hover(kind, is_promoted, false),
                    *off,
                    "for {kind:?}, promoted {is_promoted}, with the switch off"
                );
                assert_eq!(
                    variable_definition_hover(kind, is_promoted, true),
                    switched_on(kind, is_promoted, *off),
                    "for {kind:?}, promoted {is_promoted}, with the switch on"
                );
            }
        }
    }

    #[test]
    fn parameter_declaration_route_follows_promotion_alone() {
        assert_eq!(
            parameter_declaration_route(true),
            VariableDefinitionHover::ShowProperty
        );
        assert_eq!(
            parameter_declaration_route(false),
            VariableDefinitionHover::ShowVariable
        );
    }

    const PROMOTED: &str = "<?php\nclass Invoice {\n    public function __construct(private int $total, int $discount) {}\n}\n";

    fn offset_of(needle: &str) -> u32 {
        PROMOTED.find(needle).expect("needle is in PROMOTED") as u32
    }

    #[test]
    fn property_declared_at_finds_the_property_a_parameter_promotes() {
        let backend = Backend::new_test();
        backend.update_ast("file:///test.php", PROMOTED);
        let ctx = backend.file_context("file:///test.php");

        let (owner, property) =
            property_declared_at(&ctx.classes, offset_of("$total"), "total").expect("promoted");
        assert_eq!(owner.name, "Invoice");
        assert_eq!(property.name, "total");
        assert!(property_declared_at(&ctx.classes, offset_of("$discount"), "discount").is_none());
    }

    /// Class information that lags behind the buffer can hold another
    /// property at the hovered offset. Here it still records `$paid` and
    /// `$total` where the buffer renamed them to `$owed` and `$gross`,
    /// names of the same length, so every offset still matches.
    #[test]
    fn property_declared_at_needs_the_hovered_name_as_well_as_the_offset() {
        const STALE: &str = "<?php\nclass Invoice {\n    public int $paid;\n    public function __construct(private int $total) {}\n}\n";
        let backend = Backend::new_test();
        backend.update_ast("file:///test.php", STALE);
        let ctx = backend.file_context("file:///test.php");

        for (recorded, renamed) in [("paid", "owed"), ("total", "gross")] {
            let offset = STALE
                .find(&format!("${recorded}"))
                .expect("the property is in STALE") as u32;
            assert!(
                property_declared_at(&ctx.classes, offset, recorded).is_some(),
                "${recorded} is recorded at its own offset"
            );
            assert!(
                property_declared_at(&ctx.classes, offset, renamed).is_none(),
                "${renamed} must not find ${recorded}, recorded at the same offset"
            );
        }
    }

    /// Builds a state by hand: the file's symbol map stays while its
    /// class information is removed. This does not show that a session
    /// reaches that state, only that the promotion check cannot turn a
    /// promoted property into a variable hover if one did.
    #[test]
    fn constructed_promoted_parameter_without_class_information_finds_no_property() {
        let uri = "file:///test.php";
        let backend = Backend::new_test();
        backend.update_ast(uri, PROMOTED);
        backend.symbols.uri_classes_index.write().remove(uri);
        assert!(backend.shared_classes_for_uri(uri).is_none());

        let map = backend.symbol_map_for(uri).expect("the symbol map stays");
        let def = map
            .var_def_at("total", offset_of("$total"))
            .expect("the parameter is still a definition");
        assert_eq!(
            variable_definition_hover(&def.kind, def.is_promoted, true),
            VariableDefinitionHover::ShowProperty
        );
        let ctx = backend.file_context(uri);
        assert!(property_declared_at(&ctx.classes, offset_of("$total"), "total").is_none());
    }
}
