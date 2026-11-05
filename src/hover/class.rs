//! Class/interface/trait/enum hover.
//!
//! Renders the declaration signature (with `extends`/`implements`),
//! docblock description, `@see` links, template parameters, and a body
//! preview: enum cases for enums and public member signatures for
//! traits.

use std::sync::Arc;

use tower_lsp_server::gen_lsp_types::{Hover, Range};

use crate::Backend;
use crate::class_lookup::{Fqn, ResolvedClassLoader};
use crate::docblock::extract_template_params_full;
use crate::php_type::TypeKind;
use crate::symbol_map::{ClassRefContext, SelfStaticParentKind};
use crate::types::*;
use crate::util::short_name;

use super::HoverSite;
use super::formatting::*;

impl Backend {
    /// Hover for a class name the symbol map found; on `new ClassName`,
    /// the constructor's.
    pub(super) fn hover_class_symbol(
        &self,
        site: &HoverSite,
        name: &str,
        is_fqn: bool,
        context: ClassRefContext,
    ) -> Option<Hover> {
        let HoverSite {
            uri,
            content,
            offset: cursor_offset,
            loaders,
            ..
        } = *site;
        let class_loader = loaders.class_loader_at(cursor_offset);
        let resolved_class_loader = loaders.resolved_class_loader();
        // A fully qualified reference (`\stdClass`, stored without
        // its backslash) must not go through the file's imports
        // and namespace again.
        let class_loader = |name: &str| {
            if is_fqn {
                resolved_class_loader(Fqn::new(name))
            } else {
                class_loader(name)
            }
        };
        // On `new ClassName`, show the __construct method hover
        // instead of the class hover.
        if context == crate::symbol_map::ClassRefContext::New
            && let Some(cls) = class_loader(name)
        {
            let merged = crate::virtual_members::resolve_class_fully_cached(
                &cls,
                resolved_class_loader,
                &self.resolved_class_cache,
            );
            if let Some(constructor) = merged.get_method_ci("__construct") {
                return Some(self.hover_for_method(
                    constructor,
                    &merged,
                    resolved_class_loader,
                    uri,
                    content,
                ));
            }
        }

        self.hover_class_reference(name, uri, content, &class_loader, cursor_offset)
    }

    /// Hover for a class-like name at its declaration site: the class
    /// declared there, rendered exactly as a reference to it.
    ///
    /// The class is found by position, never by name, and answers only
    /// when it is the declaration its name resolves to (see
    /// [`is_resolved_class_declaration`](Self::is_resolved_class_declaration)).
    pub(super) fn hover_class_declaration(
        &self,
        site: &HoverSite,
        symbol_start: u32,
        name: &str,
    ) -> Option<Hover> {
        let class = class_declared_at(&site.ctx.classes, symbol_start).filter(|class| {
            class.name == name
                && self.is_resolved_class_declaration(
                    class,
                    site.uri,
                    site.loaders.resolved_class_loader(),
                )
        })?;
        Some(self.hover_for_class_info(class, site.uri, site.content))
    }

    /// Whether `class`, declared in `uri`, is the declaration every
    /// lookup of its name lands on: the class index holds this file for
    /// the name, no stub path declares the name in another file (a stub
    /// path wins over the index for the class loader, but not for the
    /// provenance line), and `class_loader` answers with the class
    /// written at this position.
    ///
    /// A declaration hover renders the class, and merges its inherited
    /// members, through lookups by name. Only a declaration they all
    /// agree on can be described as itself; any other declaration of
    /// the name (in another file, in another branch of a version guard,
    /// or one a stub path overrides, and that stub itself) answers
    /// nothing. An anonymous class has no index entry, but its name
    /// carries its file and offset, so a lookup by it can find no other
    /// class: when the loader misses it, the resolver keeps the class
    /// it was given.
    pub(super) fn is_resolved_class_declaration(
        &self,
        class: &ClassInfo,
        uri: &str,
        class_loader: &ResolvedClassLoader<'_>,
    ) -> bool {
        if class.name.starts_with(ANONYMOUS_CLASS_PREFIX) {
            return true;
        }
        let fqn = class.fqn();
        self.class_declaration_uri(fqn.as_str()).as_deref() == Some(uri)
            && self
                .external_stub_class_uri(fqn.as_str())
                .is_none_or(|stub_uri| stub_uri == uri)
            && class_loader(Fqn::new(fqn.as_str()))
                .is_some_and(|loaded| loaded.keyword_offset == class.keyword_offset)
    }

    /// Hover for a class-like keyword (`class`, `interface`, `trait`
    /// or `enum`) at its declaration site. The keyword has no
    /// symbol-map span, so the class is found by position, as for its
    /// name; modifiers before the keyword are not hover positions. A
    /// cursor just past the keyword still reads as on it, the same
    /// end-of-token retry the symbol-map lookup makes. The hover range
    /// is the keyword token itself. Like the name, the keyword answers
    /// only for the declaration its name resolves to.
    pub(super) fn hover_class_keyword(
        &self,
        uri: &str,
        content: &str,
        offset: u32,
    ) -> Option<Hover> {
        let ctx = self.file_context_at(uri, offset);
        let class = class_declared_at(&ctx.classes, offset).filter(|class| {
            self.is_resolved_class_declaration(class, uri, &self.resolved_class_loader(&ctx))
        })?;
        let keyword = owner_kind_keyword(class);
        let start = class.keyword_offset as usize;
        let end = start + keyword.len();
        // The keyword is checked against the text as well, in case the
        // parsed classes lag behind an edit.
        if offset as usize > end
            || !content
                .get(start..end)
                .is_some_and(|text| text.eq_ignore_ascii_case(keyword))
        {
            return None;
        }
        let mut hover = self.hover_for_class_info(class, uri, content);
        hover.range = Some(Range {
            start: crate::text_position::offset_to_position(content, start),
            end: crate::text_position::offset_to_position(content, end),
        });
        Some(hover)
    }

    /// Hover for `self`, `static`, `parent` or `$this`: the class the
    /// keyword stands for at the cursor.
    pub(super) fn hover_self_static_parent(
        &self,
        site: &HoverSite,
        ssp_kind: SelfStaticParentKind,
    ) -> Option<Hover> {
        let HoverSite {
            uri,
            content,
            offset: cursor_offset,
            loaders,
            ..
        } = *site;
        let current_class = site.current_class();
        let resolved_class_loader = loaders.resolved_class_loader();
        let is_this = ssp_kind == SelfStaticParentKind::This;

        let resolved = match ssp_kind {
            SelfStaticParentKind::Self_ | SelfStaticParentKind::Static => current_class.cloned(),
            SelfStaticParentKind::This => {
                let bound = self.resolve_closure_this_types(uri, content, cursor_offset);
                if bound.len() > 1 {
                    return Some(make_hover(bound_this_hover(&bound)));
                }
                bound
                    .into_iter()
                    .next()
                    .and_then(ResolvedType::into_class_info)
                    .map(Arc::unwrap_or_clone)
                    .or_else(|| current_class.cloned())
            }
            SelfStaticParentKind::Parent => current_class
                .and_then(|cc| cc.parent_class.as_ref())
                .and_then(|parent_name| {
                    resolved_class_loader(Fqn::new(parent_name)).map(Arc::unwrap_or_clone)
                }),
        };
        if let Some(cls) = resolved {
            let mut lines = Vec::new();

            if let Some(desc) = extract_docblock_description(cls.class_docblock.as_deref()) {
                lines.push(desc);
            }

            if let Some(ref msg) = cls.deprecation_message {
                lines.push(format_deprecation_line(msg));
            }

            let ns_line = namespace_line(cls.file_namespace.as_deref());
            if is_this {
                lines.push(format!(
                    "```php\n<?php\n{}$this = {}\n```",
                    ns_line, cls.name
                ));
            } else {
                let keyword_str = match ssp_kind {
                    SelfStaticParentKind::Self_ => "self",
                    SelfStaticParentKind::Static => "static",
                    SelfStaticParentKind::Parent => "parent",
                    SelfStaticParentKind::This => unreachable!(),
                };
                lines.push(format!(
                    "```php\n<?php\n{}{} = {}\n```",
                    ns_line, keyword_str, cls.name
                ));
            }

            Some(make_hover(lines.join("\n\n")))
        } else {
            None
        }
    }

    /// Produce hover information for a class reference.
    pub(super) fn hover_class_reference(
        &self,
        name: &str,
        uri: &str,
        content: &str,
        class_loader: &dyn Fn(&str) -> Option<std::sync::Arc<ClassInfo>>,
        cursor_offset: u32,
    ) -> Option<Hover> {
        // In a docblock a template parameter in scope shadows a class of
        // the same name, so `@param T $x` under `@template T` names the
        // template even when a class `T` exists.  In code it is the class.
        if crate::completion::source::comment_position::is_offset_inside_docblock(
            content,
            cursor_offset as usize,
        ) && let Some(tpl) = self.find_template_def_for_hover(uri, name, cursor_offset)
        {
            return Some(tpl);
        }

        if let Some(cls) = class_loader(name) {
            Some(self.hover_for_class_info(&cls, uri, content))
        } else {
            // Check whether this is a template parameter in scope.
            self.find_template_def_for_hover(uri, name, cursor_offset)
        }
    }

    /// Build hover content for a class/interface/trait/enum.
    pub(crate) fn hover_for_class_info(&self, cls: &ClassInfo, uri: &str, content: &str) -> Hover {
        let kind_str = class_header_keyword(cls);

        let mut extends_implements = String::new();

        // For interfaces, `parent_class` is the first element of
        // `interfaces` (both come from the same `extends` clause),
        // so skip it to avoid duplicating the name.
        if cls.kind != ClassLikeKind::Interface
            && let Some(ref parent) = cls.parent_class
        {
            extends_implements.push_str(&format!(" extends {}", short_name(parent)));
        }

        if !cls.interfaces.is_empty() {
            let keyword = if cls.kind == ClassLikeKind::Interface {
                "extends"
            } else {
                "implements"
            };
            let short_ifaces: Vec<&str> = cls.interfaces.iter().map(|i| short_name(i)).collect();
            extends_implements.push_str(&format!(" {} {}", keyword, short_ifaces.join(", ")));
        }

        let signature = format!("{} {}{}", kind_str, cls.name, extends_implements);
        let ns_line = namespace_line(cls.file_namespace.as_deref());

        let mut lines = Vec::new();

        if let Some(desc) = extract_docblock_description(cls.class_docblock.as_deref()) {
            lines.push(desc);
        }

        if let Some(ref msg) = cls.deprecation_message {
            lines.push(format_deprecation_line(msg));
        }

        for url in &cls.links {
            lines.push(format!("[{}]({})", url, url));
        }

        let resolved_see = self.resolve_see_refs(&cls.see_refs, uri, content);
        format_see_refs(&resolved_see, &cls.links, &mut lines);

        // Show template parameters with variance and bounds.
        if let Some(ref docblock) = cls.class_docblock {
            let tpl_entries: Vec<String> = extract_template_params_full(docblock)
                .into_iter()
                .map(|(name, bound, variance, default)| {
                    let bound_display = bound
                        .map(|b| format!(" of `{}`", b.shorten()))
                        .unwrap_or_default();
                    let default_display =
                        default.map(|d| format!(" = `{}`", d)).unwrap_or_default();
                    format!(
                        "**{}** `{}`{}{}",
                        variance.tag_name(),
                        name,
                        bound_display,
                        default_display
                    )
                })
                .collect();
            if !tpl_entries.is_empty() {
                lines.push(tpl_entries.join("  \n"));
            }
        }

        let body_lines = if cls.kind == ClassLikeKind::Enum {
            build_enum_case_body(cls)
        } else if cls.kind == ClassLikeKind::Trait {
            build_trait_summary_body(cls)
        } else {
            String::new()
        };

        if body_lines.is_empty() {
            lines.push(format!("```php\n<?php\n{}{}\n```", ns_line, signature));
        } else {
            lines.push(format!(
                "```php\n<?php\n{}{} {{\n{}}}\n```",
                ns_line, signature, body_lines
            ));
        }

        if let Some(prov) = self.provenance_line_for_class(&cls.fqn()) {
            lines.push(prov);
        }

        make_hover(lines.join("\n\n"))
    }
}

/// The named class-like whose declaration header, from its keyword up
/// to its body's opening brace, contains `offset`. Anonymous classes
/// record no keyword offset and never match.
fn class_declared_at(classes: &[Arc<ClassInfo>], offset: u32) -> Option<&ClassInfo> {
    classes.iter().map(Arc::as_ref).find(|class| {
        class.keyword_offset != 0 && (class.keyword_offset..class.start_offset).contains(&offset)
    })
}

/// Maximum number of enum cases or trait methods to show before
/// truncating with a "and N more…" comment.
const MAX_BODY_ITEMS: usize = 30;

/// Build the body lines for an enum hover showing its cases.
///
/// Only enum cases are shown (not regular class constants).
/// Each case is rendered as `    case Name = 'value';` or `    case Name;`.
/// If there are more than [`MAX_BODY_ITEMS`] cases, the list is truncated
/// with a `// and N more…` comment.
fn build_enum_case_body(cls: &ClassInfo) -> String {
    let cases: Vec<&ConstantInfo> = cls
        .constants
        .iter()
        .filter(|c| c.is_enum_case)
        .map(|c| c.as_ref())
        .collect();

    if cases.is_empty() {
        return String::new();
    }

    let mut body = String::new();
    let shown = cases.len().min(MAX_BODY_ITEMS);

    for case in &cases[..shown] {
        if let Some(ref val) = case.enum_value {
            body.push_str(&format!("    case {} = {};\n", case.name, val));
        } else {
            body.push_str(&format!("    case {};\n", case.name));
        }
    }

    if cases.len() > MAX_BODY_ITEMS {
        body.push_str(&format!(
            "    // and {} more…\n",
            cases.len() - MAX_BODY_ITEMS
        ));
    }

    body
}

/// Build the body lines for a trait hover showing public member signatures.
///
/// Shows public methods (one-line signatures without bodies), public
/// properties, and public constants. Uses native types only and short
/// (unqualified) class names for a scannable summary.
///
/// If there are more than [`MAX_BODY_ITEMS`] members, the list is
/// truncated with a `// and N more…` comment.
fn build_trait_summary_body(cls: &ClassInfo) -> String {
    let mut member_lines: Vec<String> = Vec::new();

    for constant in &cls.constants {
        if constant.visibility != Visibility::Public {
            continue;
        }
        member_lines.push(format!("    {}", constant_declaration(constant)));
    }

    for prop in &cls.properties {
        if prop.visibility != Visibility::Public {
            continue;
        }
        let static_kw = if prop.is_static { "static " } else { "" };
        let readonly_kw = if prop.is_readonly { "readonly " } else { "" };
        let native_type = prop
            .native_type_hint
            .as_ref()
            .map(|t| format!("{} ", t))
            .unwrap_or_default();
        member_lines.push(format!(
            "    public {}{}{}${};",
            static_kw, readonly_kw, native_type, prop.name
        ));
    }

    for method in &cls.methods {
        if method.visibility != Visibility::Public {
            continue;
        }
        let static_kw = if method.is_static { "static " } else { "" };
        let modifier_kw = method_modifier_keyword(method, cls);
        let native_params = format_native_params(&method.parameters);
        let native_ret = method
            .native_return_type
            .as_ref()
            .map(|r| format!(": {}", r))
            .unwrap_or_default();
        member_lines.push(format!(
            "    {}public {}function {}({}){};",
            modifier_kw, static_kw, method.name, native_params, native_ret
        ));
    }

    if member_lines.is_empty() {
        return String::new();
    }

    let shown = member_lines.len().min(MAX_BODY_ITEMS);
    let mut body: String = member_lines[..shown].join("\n");
    body.push('\n');

    if member_lines.len() > MAX_BODY_ITEMS {
        body.push_str(&format!(
            "    // and {} more…\n",
            member_lines.len() - MAX_BODY_ITEMS
        ));
    }

    body
}

/// Hover text for `$this` bound to several classes, as a union or an
/// intersection, named relative to their namespace when they share one.
fn bound_this_hover(bound: &[ResolvedType]) -> String {
    let classes: Vec<&ClassInfo> = bound
        .iter()
        .filter_map(|rt| rt.class_info.as_deref())
        .collect();
    // The members of an intersection binding all carry the intersection
    // as their `type_string`; anything else is a union of alternatives.
    let separator = match bound.first().map(|rt| &rt.type_string) {
        Some(first)
            if matches!(first.kind(), TypeKind::Intersection(_))
                && bound.iter().all(|rt| &rt.type_string == first) =>
        {
            "&"
        }
        _ => "|",
    };
    let namespace = classes
        .first()
        .and_then(|cls| cls.file_namespace.as_deref());
    let shared_namespace = classes
        .iter()
        .all(|cls| cls.file_namespace.as_deref() == namespace);
    let names: Vec<String> = if shared_namespace {
        classes.iter().map(|cls| cls.name.to_string()).collect()
    } else {
        classes
            .iter()
            .map(|cls| format!("\\{}", cls.fqn()))
            .collect()
    };
    let ns_line = if shared_namespace {
        namespace_line(namespace)
    } else {
        String::new()
    };
    format!(
        "```php\n<?php\n{}$this = {}\n```",
        ns_line,
        names.join(separator)
    )
}
