//! Function hover: the signature and docblock of a called function, and
//! the member an unqualified `@see name()` names on the documented class.

use tower_lsp_server::gen_lsp_types::Hover;

use crate::Backend;
use crate::types::FunctionInfo;

use super::HoverSite;
use super::formatting::hover_for_function;
use super::member::HoverMemberHit;

impl Backend {
    /// Hover for a function call or a `@see name()` reference.
    pub(super) fn hover_function_symbol(
        &self,
        site: &HoverSite,
        symbol_start: u32,
        name: &str,
        is_docblock_reference: bool,
    ) -> Option<Hover> {
        let HoverSite {
            uri,
            content,
            offset: cursor_offset,
            ctx,
            loaders,
        } = *site;
        let resolved_class_loader = loaders.resolved_class_loader();
        let function_loader = loaders.function_loader_at(cursor_offset);
        // An unqualified `@see name()` describes the documented
        // class's own member first, as phpDocumentor reads it.
        if is_docblock_reference
            && let Some((class, kind)) = self.docblock_scope_member(ctx, symbol_start, name)
            && let Some(hit) = Self::find_member_for_hover(&class, name, kind.into())
        {
            return Some(match hit {
                HoverMemberHit::Method(m) => {
                    self.hover_for_method(&m, &class, &resolved_class_loader, uri, content)
                }
                HoverMemberHit::Property(p) => {
                    self.hover_for_property(&p, &class, &resolved_class_loader)
                }
                HoverMemberHit::Constant(c) => {
                    self.hover_for_constant(&c, &class, &resolved_class_loader)
                }
            });
        }

        self.hover_function_call(name, uri, content, function_loader, symbol_start)
    }

    /// Hover for a function name at its declaration site: the function
    /// this file declares at this very position, under its fully
    /// qualified name (its namespace block plus its stored name).
    ///
    /// The declaration answers only when every lookup of its name lands
    /// on it: the function index holds it, because body inference and
    /// the provenance line read that entry, and no stub path declares
    /// the name in another file, because a call resolves to the stub
    /// path's declaration first. Any other declaration of the name (in
    /// another file, in another branch of this one, or one a stub path
    /// overrides, and that stub itself) answers nothing. The rendering
    /// is a call's, so inferred return types, `@see` links, provenance
    /// and the configured date-helper override all match a call.
    pub(super) fn hover_function_declaration(
        &self,
        site: &HoverSite,
        symbol_start: u32,
        name: &str,
    ) -> Option<Hover> {
        let fqn = declared_global_name(site.ctx.namespace_at(site.offset).as_deref(), name);
        if self
            .external_stub_function_uri(&fqn)
            .is_some_and(|stub_uri| stub_uri != site.uri)
        {
            return None;
        }
        let mut func = self
            .symbols
            .global_functions
            .read()
            .get(&fqn)
            .filter(|(declaring_uri, func)| {
                declaring_uri == site.uri && func.name_offset == symbol_start
            })
            .map(|(_, func)| func.clone())?;
        self.verify_function_doc_types(&mut func);
        Some(self.function_hover(&fqn, func, site.uri, site.content))
    }

    /// Produce hover information for a function call.
    fn hover_function_call(
        &self,
        name: &str,
        uri: &str,
        content: &str,
        function_loader: &dyn Fn(&str, u32) -> Option<FunctionInfo>,
        offset: u32,
    ) -> Option<Hover> {
        function_loader(name, offset).map(|func| self.function_hover(name, func, uri, content))
    }

    /// The hover for `func`, called or declared as `name`.
    fn function_hover(
        &self,
        name: &str,
        mut func: FunctionInfo,
        uri: &str,
        content: &str,
    ) -> Hover {
        let is_configured_date_helper = matches!(
            name.trim_start_matches('\\').rsplit('\\').next(),
            Some("now" | "today")
        );
        // Only when the configured date class actually resolves do we
        // override the declared return type; the `(inferred)` annotation
        // must reflect that override, not merely the helper's name (a
        // user-defined `now()` in a non-Laravel project keeps its own
        // return type unannotated).
        let mut inferred_return = false;
        if is_configured_date_helper
            && let Some(date_class) =
                self.find_or_load_class(crate::virtual_members::laravel::CONFIGURED_DATE_CLASS_FQN)
        {
            let date_type = crate::php_type::PhpType::named(date_class.fqn());
            func.return_type = Some(date_type);
            inferred_return = true;
        } else if func.return_type.is_none()
            && let Some(inferred) =
                crate::type_engine::call_resolution::try_infer_function_body_return_type(
                    self, &func,
                )
        {
            func.return_type = Some(inferred);
            inferred_return = true;
        }
        let resolved_see = self.resolve_see_refs(&func.see_refs, uri, content);
        let provenance = self.provenance_line_for_function(name);
        hover_for_function(&func, Some(&resolved_see), provenance, inferred_return)
    }
}

/// The fully qualified name a declaration of `name` in `namespace`
/// declares: the namespace plus the stored name, or the stored name
/// alone in the global namespace.
pub(super) fn declared_global_name(namespace: Option<&str>, name: &str) -> String {
    match namespace {
        Some(namespace) if !namespace.is_empty() => format!("{namespace}\\{name}"),
        _ => name.to_string(),
    }
}
