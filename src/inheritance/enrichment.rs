//! Docblock enrichment: propagating type information from ancestors.
//!
//! When a child class lacks docblock-provided type information (return types,
//! parameter types, descriptions), this module propagates richer type information
//! from ancestors through inheritance chains.
//!
//! "The child wrote no PHPDoc type" and "the ancestor supplies one" are read
//! from the stored facts (`doc_type`, `doc_return_type`), never guessed from
//! the effective and native types: an explicit `@param int $id` on a native
//! `int $id` leaves the two equal and still overrides the ancestor.  A child
//! that inherits a type inherits the ancestor's doc type with it, and the
//! merge chain visits the nearest ancestor first, so the nearest documented
//! ancestor wins and a grandparent's tag still reaches an undocumented
//! grandchild through an undocumented parent.

use std::sync::Arc;

use super::ancestry::ClassLoader;
use crate::docblock::doc_type_widens_native;
use crate::php_type::{PhpType, TypeKind};
use crate::types::{MethodInfo, ParameterInfo, PropertyInfo};

/// The return type `existing` should take from `ancestor`, or `None` when
/// it keeps its own.
///
/// The ancestor's type applies when (a) the child has no return type at
/// all, or (b) no PHPDoc return type applies to the child
/// (`doc_return_type` is `None`) and one applies to the ancestor. The
/// child's *native* hint still has the last word: an override
/// declaring `: array` cannot return the `string` half of an interface's
/// `@return array|string`, so the inherited union is restricted to what the
/// override's own declaration allows, and a type wider than that
/// declaration gives way to it ([`doc_type_widens_native`]). A native `never`
/// allows nothing to be returned, so there is nothing an ancestor's
/// docblock could refine.
fn inherited_return_type(
    existing: &MethodInfo,
    ancestor: &MethodInfo,
    class_loader: ClassLoader<'_>,
) -> Option<PhpType> {
    if existing
        .native_return_type
        .as_ref()
        .is_some_and(PhpType::is_never)
    {
        return None;
    }
    if !(existing.return_type.is_none() && ancestor.return_type.is_some()
        || existing.doc_return_type.is_none() && ancestor.doc_return_type.is_some())
    {
        return None;
    }

    let inherited = in_override_param_names(ancestor.return_type.as_ref()?, existing, ancestor);
    let Some(ref native) = existing.native_return_type else {
        return Some(inherited);
    };
    // The native hint retains source spellings for display. With no own
    // doc type, the effective type is that hint with class names resolved;
    // a cap that copies one of its alternatives must keep those FQNs.
    let native = existing.return_type.as_ref().unwrap_or(native);
    let inherited = within_native_class(inherited, existing, ancestor, class_loader)?;
    let inherited = inherited.without_alternatives_the_native_type_forbids(native)?;
    (!doc_type_widens_native(&inherited, native)).then_some(inherited)
}

/// `inherited` without the classes the override's own native return hint
/// rules out, or `None` when nothing but `null` is left.
///
/// An override declaring `: View` can only return a `View`, so a parent's
/// `@return View|Htmlable|\Closure` keeps just `View`, and an interface's
/// `@return Factory<Meta>` over an override narrowing to `: OrmFactory` (a
/// `Factory` subclass) offers nothing the override can use, so the native
/// hint wins outright, as it does in PHPStan. An alternative naming the
/// override's class or one of its subclasses still refines it, and one
/// that cannot be classified (a template, `static`, a class that does not
/// load) is kept, since dropping it could not be proved right.
///
/// The ancestor's class-level templates are already substituted (or
/// bound-filled) by the caller, but its method-level ones reach here as
/// bare names, so those are checked against `ancestor` before the loader
/// gets a chance to mistake `T` for an unrelated global class `T`.
fn within_native_class(
    inherited: PhpType,
    existing: &MethodInfo,
    ancestor: &MethodInfo,
    class_loader: ClassLoader<'_>,
) -> Option<PhpType> {
    // With no docblock of its own, the effective return type is the native
    // hint with its class name already resolved to a FQN.
    let Some(own) = existing.return_type.as_ref() else {
        return Some(inherited);
    };
    let own = own.non_null_type().unwrap_or_else(|| own.clone());
    // `mixed` is not in the scalar keyword list, so `class_name` lets it by.
    let Some(own_class) = own.class_name().filter(|_| !own.is_mixed()) else {
        return Some(inherited);
    };
    let admits = |member: &PhpType| {
        if !matches!(member.kind(), TypeKind::Named(_) | TypeKind::Generic(_))
            || member.is_self_like()
        {
            return true;
        }
        let Some(class) = member.base_name() else {
            return true;
        };
        if ancestor
            .template_params()
            .iter()
            .any(|t| t.as_str() == class)
        {
            return true;
        }
        match class_loader(crate::class_lookup::Fqn::new(class)) {
            Some(info) => crate::class_lookup::is_subtype_of(&info, own_class, class_loader),
            None => true,
        }
    };

    let members = inherited.union_members();
    if members.iter().all(|member| admits(member)) {
        return Some(inherited);
    }
    let kept: Vec<PhpType> = members
        .into_iter()
        .filter(|member| admits(member))
        .cloned()
        .collect();
    if kept.iter().all(PhpType::is_null) {
        return None;
    }
    Some(PhpType::union(kept))
}

/// `ty`, written in `ancestor`'s docblock, with each conditional that names
/// one of `ancestor`'s parameters renamed to the parameter `existing`
/// declares in the same position.
fn in_override_param_names(ty: &PhpType, existing: &MethodInfo, ancestor: &MethodInfo) -> PhpType {
    if !ty.contains_conditional() {
        return ty.clone();
    }
    ty.rename_conditional_params(&|name| {
        let position = ancestor.parameters.iter().position(|p| p.name == name)?;
        let renamed = existing.parameters.get(position)?.name;
        (renamed != name).then_some(renamed)
    })
}

/// Copy-on-write wrapper around [`enrich_method_from_ancestor`].
///
/// Most class/ancestor method overlaps have nothing to propagate (the
/// child already carries its own docblock types, or the ancestor has
/// nothing richer to offer).  Checking first means the shared
/// `Arc<MethodInfo>` is only deep-cloned when the enrichment actually
/// changes a field, instead of once per overlap per class.
pub(crate) fn enrich_method_arc_from_ancestor(
    existing: &mut Arc<MethodInfo>,
    ancestor: &MethodInfo,
    class_loader: ClassLoader<'_>,
) {
    if method_enrichment_would_change(existing, ancestor, class_loader) {
        enrich_method_from_ancestor(Arc::make_mut(existing), ancestor, class_loader);
    }
}

/// Whether [`enrich_method_from_ancestor`] would change any field of
/// `existing`.
///
/// Mirrors the apply function's conditions exactly (keep the two in
/// sync), with one refinement: a copy that would overwrite a field with
/// an equal value counts as "no change".  A `false` here guarantees the
/// enrichment is semantically a no-op, so the caller can skip the
/// copy-on-write clone.
fn method_enrichment_would_change(
    existing: &MethodInfo,
    ancestor: &MethodInfo,
    class_loader: ClassLoader<'_>,
) -> bool {
    // Return type, and the doc type that comes with it.
    if let Some(inherited) = inherited_return_type(existing, ancestor, class_loader)
        && (existing.return_type.as_ref() != Some(&inherited)
            || existing.doc_return_type != ancestor.doc_return_type)
    {
        return true;
    }

    // Template parameters (copies bounds, defaults and bindings alongside).
    if existing.template_params().is_empty() && !ancestor.template_params().is_empty() {
        return true;
    }

    // Conditional return type.
    if existing.conditional_return().is_none() && ancestor.conditional_return().is_some() {
        return true;
    }

    // Type assertions.
    if existing.type_assertions().is_empty() && !ancestor.type_assertions().is_empty() {
        return true;
    }

    // Parameters (constructors match by name, everything else by position).
    let params_would_change = if existing.name == "__construct" {
        constructor_parameters_would_change(&existing.parameters, &ancestor.parameters)
    } else {
        positional_parameters_would_change(&existing.parameters, &ancestor.parameters)
    };
    if params_would_change {
        return true;
    }

    // Deprecation (an override with no docblock of its own inherits the
    // ancestor's `@deprecated` tag, matching PHPStan's phpDoc-inheritance
    // resolver).
    if !existing.not_deprecated()
        && existing.deprecation_message().is_none()
        && ancestor.deprecation_message().is_some()
    {
        return true;
    }

    // Descriptions.
    (existing.description.is_none() && ancestor.description.is_some())
        || inherits_return_description(existing, ancestor)
}

/// Whether [`enrich_constructor_parameters_by_name`] would change any
/// parameter.
fn constructor_parameters_would_change(
    existing_params: &[ParameterInfo],
    ancestor_params: &[ParameterInfo],
) -> bool {
    existing_params.iter().any(|ep| {
        ancestor_params
            .iter()
            .find(|ap| ap.name == ep.name)
            .is_some_and(|ap| parameter_enrichment_would_change(ep, ap))
    })
}

/// Whether [`enrich_parameters_from_ancestor`] would change any
/// parameter.
fn positional_parameters_would_change(
    existing_params: &[ParameterInfo],
    ancestor_params: &[ParameterInfo],
) -> bool {
    existing_params
        .iter()
        .zip(ancestor_params)
        .any(|(ep, ap)| parameter_enrichment_would_change(ep, ap))
}

/// Whether the child's own native parameter hint supersedes the
/// ancestor's docblock type.
///
/// PHP requires an override to restate every parameter's type hint, so a
/// child that repeats the ancestor's hint verbatim has declared nothing
/// new: the ancestor's `@param` docblock (with template substitutions
/// from `@extends`/`@implements` already applied) still describes the
/// value that reaches the body.  That is what narrows a
/// `@implements Rule<CallLike>` implementation's `Node $node` parameter
/// to `CallLike`, matching how PHPStan inherits PHPDoc.
///
/// A child that names a *different* specific type has made a deliberate
/// declaration — parameters are contravariant, so it may legitimately be
/// wider than the ancestor's — and keeps it.
fn child_native_hint_overrides(
    existing_param: &ParameterInfo,
    ancestor_param: &ParameterInfo,
) -> bool {
    existing_param.native_type_hint != ancestor_param.native_type_hint
        && existing_param.native_type_hint.as_ref().is_some_and(|nt| {
            !nt.is_object() && !nt.is_mixed() && !nt.is_array_like() && !nt.is_iterable()
        })
}

/// Whether `existing_param` takes its type (and doc type) from
/// `ancestor_param`: no PHPDoc type applies to the child, one applies to
/// the ancestor, the child declared no specific native hint of its own,
/// and the ancestor's type is not wider than the child's native hint
/// ([`doc_type_widens_native`]).
fn inherits_parameter_type(existing_param: &ParameterInfo, ancestor_param: &ParameterInfo) -> bool {
    !child_native_hint_overrides(existing_param, ancestor_param)
        && existing_param.doc_type.is_none()
        && ancestor_param.doc_type.is_some()
        && !widens_own_native(
            ancestor_param.type_hint.as_ref(),
            existing_param.native_type_hint.as_ref(),
        )
}

/// [`doc_type_widens_native`] for a type and a native hint that may be
/// absent: no native hint is nothing to widen.
fn widens_own_native(inherited: Option<&PhpType>, native: Option<&PhpType>) -> bool {
    inherited
        .zip(native)
        .is_some_and(|(inherited, native)| doc_type_widens_native(inherited, native))
}

/// Whether `existing` takes its type (and doc type) from `ancestor`: the
/// child has no type at all, or no PHPDoc type applies to it, one applies
/// to the ancestor, and that type is not wider than the child's native
/// hint ([`doc_type_widens_native`]).
fn inherits_property_type(existing: &PropertyInfo, ancestor: &PropertyInfo) -> bool {
    existing.type_hint.is_none() && ancestor.type_hint.is_some()
        || existing.doc_type.is_none()
            && ancestor.doc_type.is_some()
            && !widens_own_native(
                ancestor.type_hint.as_ref(),
                existing.native_type_hint.as_ref(),
            )
}

/// Whether `existing_param` takes `ancestor_param`'s description: it has
/// none, and wrote no `@param` type, whose tag would be the element whose
/// own (empty) text wins (D-10).
fn inherits_parameter_description(
    existing_param: &ParameterInfo,
    ancestor_param: &ParameterInfo,
) -> bool {
    existing_param.description.is_none()
        && existing_param.doc_type.is_none()
        && ancestor_param.description.is_some()
}

/// Whether `existing` takes `ancestor`'s `@return` description: the same
/// rule as [`inherits_parameter_description`].
fn inherits_return_description(existing: &MethodInfo, ancestor: &MethodInfo) -> bool {
    existing.return_description().is_none()
        && existing.doc_return_type.is_none()
        && ancestor.return_description().is_some()
}

/// Whether `existing` takes `ancestor`'s description: the same rule as
/// [`inherits_parameter_description`], for the property's `@var`.
fn inherits_property_description(existing: &PropertyInfo, ancestor: &PropertyInfo) -> bool {
    existing.description.is_none() && existing.doc_type.is_none() && ancestor.description.is_some()
}

/// Whether [`enrich_single_parameter`] would change `existing_param`.
///
/// Mirrors the apply function's conditions (keep the two in sync).
fn parameter_enrichment_would_change(
    existing_param: &ParameterInfo,
    ancestor_param: &ParameterInfo,
) -> bool {
    if inherits_parameter_type(existing_param, ancestor_param) {
        return true;
    }
    existing_param.callable_invocation.is_none() && ancestor_param.callable_invocation.is_some()
        || existing_param.param_out_type.is_none() && ancestor_param.param_out_type.is_some()
        || inherits_parameter_description(existing_param, ancestor_param)
}

/// Enrich a child method with docblock information from an ancestor method.
///
/// Propagates return types, parameter types, descriptions, template
/// parameters, conditional return types, and type assertions from the
/// ancestor when the child lacks its own docblock overrides.
///
/// **Return type rule:** If no PHPDoc return type applies to the child
/// (`doc_return_type` is `None`) and one applies to the ancestor, copy the
/// ancestor's `return_type` and `doc_return_type` to the child.  If the
/// child has no `return_type` at all, always inherit the ancestor's.
///
/// **Parameter rule:** Match by position (not by name, since the child
/// may rename parameters).  The same doc-type rule as return types.
///
/// **Description rule:** Inherit `description` when the child has `None`,
/// and `return_description` when the child has `None` and wrote no
/// `@return` type: a written tag is the element, and its own (possibly
/// empty) text wins (D-10).  The same holds for each parameter.
pub(crate) fn enrich_method_from_ancestor(
    existing: &mut MethodInfo,
    ancestor: &MethodInfo,
    class_loader: ClassLoader<'_>,
) {
    // Asked before the return type below copies the ancestor's doc type.
    let takes_return_description = inherits_return_description(existing, ancestor);

    // ── Return type ─────────────────────────────────────────────
    if let Some(inherited) = inherited_return_type(existing, ancestor, class_loader) {
        existing.return_type = Some(inherited);
        existing.doc_return_type = ancestor.doc_return_type.clone();
    }

    // ── Template parameters ─────────────────────────────────────
    if existing.template_params().is_empty() && !ancestor.template_params().is_empty() {
        let extras = existing.extras_mut();
        extras.template_params = ancestor.template_params().to_vec();
        extras.template_param_bounds = ancestor.template_param_bounds().clone();
        extras.template_param_defaults = ancestor.template_param_defaults().into();
        extras.template_bindings = ancestor.template_bindings().to_vec();
        // Template return types like `T` only make sense when the
        // template params are present — inherit the return type too
        // if we haven't already set it.
        if existing.return_type.is_none() {
            existing.return_type = ancestor.return_type.clone();
        }
    }

    // ── Conditional return type ─────────────────────────────────
    if existing.conditional_return().is_none()
        && let Some(conditional) = ancestor.conditional_return()
    {
        let conditional = in_override_param_names(conditional, existing, ancestor);
        existing.extras_mut().conditional_return = Some(conditional);
    }

    // ── Type assertions ─────────────────────────────────────────
    if existing.type_assertions().is_empty() && !ancestor.type_assertions().is_empty() {
        existing.extras_mut().type_assertions = ancestor.type_assertions().to_vec();
    }

    // ── Parameters ──────────────────────────────────────────────
    // For constructors, use **name-based** matching instead of
    // positional.  PHP constructors don't follow Liskov substitution
    // — a child constructor can have a completely different signature
    // (different parameter count, order, types).  Positional
    // enrichment would incorrectly map ancestor param types onto
    // unrelated child params (e.g. Exception's `$code` type `int`
    // onto a child's `$message` param at position 1).
    //
    // This follows PHPStan's `PhpDocInheritanceResolver`: for
    // `__construct` the positional parameter name list falls back to
    // the child's own names, so only same-named parameters inherit.
    // The would-change pre-check keeps the shared parameter list intact
    // (no copy-on-write) when there is nothing to propagate.
    if existing.name == "__construct" {
        if constructor_parameters_would_change(&existing.parameters, &ancestor.parameters) {
            enrich_constructor_parameters_by_name(
                existing.parameters.make_mut(),
                &ancestor.parameters,
            );
        }
    } else if positional_parameters_would_change(&existing.parameters, &ancestor.parameters) {
        enrich_parameters_from_ancestor(existing.parameters.make_mut(), &ancestor.parameters);
    }

    // ── Deprecation ─────────────────────────────────────────────
    if !existing.not_deprecated()
        && existing.deprecation_message().is_none()
        && ancestor.deprecation_message().is_some()
    {
        let extras = existing.extras_mut();
        extras.deprecation_message = ancestor.deprecation_message().map(str::to_owned);
        extras.deprecated_replacement = ancestor.deprecated_replacement().map(str::to_owned);
    }

    // ── Descriptions ────────────────────────────────────────────
    if existing.description.is_none() && ancestor.description.is_some() {
        existing.description = ancestor.description.clone();
    }
    if takes_return_description {
        existing.extras_mut().return_description = ancestor.return_description().map(str::to_owned);
    }
}

/// Enrich child parameters from ancestor parameters, matched by position.
///
/// When no PHPDoc type applies to a child parameter and one applies to the
/// ancestor parameter, copy the ancestor's `type_hint` and `doc_type`.
/// Also inherit `description` when the child parameter has none.
pub(crate) fn enrich_parameters_from_ancestor(
    existing_params: &mut [ParameterInfo],
    ancestor_params: &[ParameterInfo],
) {
    for (existing_param, ancestor_param) in existing_params.iter_mut().zip(ancestor_params) {
        enrich_single_parameter(existing_param, ancestor_param);
    }
}

/// Enrich constructor parameters from ancestor parameters, matched by name.
///
/// Unlike regular methods (which follow Liskov substitution and can
/// safely use positional matching), constructors can have completely
/// different signatures.  Only parameters with the **same name** in
/// both the child and ancestor are enriched.
pub(crate) fn enrich_constructor_parameters_by_name(
    existing_params: &mut [ParameterInfo],
    ancestor_params: &[ParameterInfo],
) {
    for existing_param in existing_params.iter_mut() {
        if let Some(ancestor_param) = ancestor_params
            .iter()
            .find(|ap| ap.name == existing_param.name)
        {
            enrich_single_parameter(existing_param, ancestor_param);
        }
    }
}

/// Enrich a single child parameter from an ancestor parameter.
///
/// Copies the ancestor's `type_hint` and `doc_type` when no PHPDoc type
/// applies to the child, one applies to the ancestor, **and** the child's
/// native type is not a specific concrete type.
///
/// PHP allows contravariant parameter types: a concrete class may
/// declare `?int` where the interface says `int`, or Carbon's
/// `setTimezone(DateTimeZone|string|int)` may widen DateTime's
/// `setTimezone(DateTimeZone)`.  In those cases the child's native
/// type is an intentional widening and must not be narrowed.
///
/// However, when the child's native type is a placeholder like
/// `object` or `mixed` (common in `@implements`/`@extends` generic
/// patterns where the interface declares `object $entity` and the
/// `@implements` tag substitutes the template to a concrete type),
/// the ancestor's enriched type should flow through.
pub(crate) fn enrich_single_parameter(
    existing_param: &mut ParameterInfo,
    ancestor_param: &ParameterInfo,
) {
    // Asked before the type below copies the ancestor's doc type.
    let takes_description = inherits_parameter_description(existing_param, ancestor_param);
    if inherits_parameter_type(existing_param, ancestor_param) {
        existing_param.type_hint = ancestor_param.type_hint.clone();
        existing_param.doc_type = ancestor_param.doc_type.clone();
    }
    // An override that does not say when it runs a callable runs it when
    // the method it overrides promised to, the way PHPStan inherits the
    // invocation tags.
    if existing_param.callable_invocation.is_none() {
        existing_param.callable_invocation = ancestor_param.callable_invocation;
    }
    // Likewise for what it writes through a by-reference parameter.
    if existing_param.param_out_type.is_none() {
        existing_param.param_out_type = ancestor_param.param_out_type.clone();
    }
    // Description enrichment
    if takes_description {
        existing_param.description = ancestor_param.description.clone();
    }
}

/// Copy-on-write wrapper around [`enrich_property_from_ancestor`].
///
/// The property analogue of [`enrich_method_arc_from_ancestor`]: the
/// shared `Arc<PropertyInfo>` is only deep-cloned when the enrichment
/// would actually change a field, which keeps inherited properties
/// sharing their allocation in the common no-op case.
pub(crate) fn enrich_property_arc_from_ancestor(
    existing: &mut Arc<PropertyInfo>,
    ancestor: &PropertyInfo,
) {
    if property_enrichment_would_change(existing, ancestor) {
        enrich_property_from_ancestor(Arc::make_mut(existing), ancestor);
    }
}

/// Whether [`enrich_property_from_ancestor`] would change any field of
/// `existing`.  Mirrors the apply function's conditions exactly (keep the
/// two in sync); an overwrite with an equal value counts as "no change".
fn property_enrichment_would_change(existing: &PropertyInfo, ancestor: &PropertyInfo) -> bool {
    // Type hint, and the doc type that comes with it.
    if inherits_property_type(existing, ancestor)
        && (existing.type_hint != ancestor.type_hint || existing.doc_type != ancestor.doc_type)
    {
        return true;
    }

    // Description.
    inherits_property_description(existing, ancestor)
}

/// Enrich a child property with docblock information from an ancestor
/// property.
///
/// Propagates type hints and descriptions from the ancestor when the
/// child lacks its own.  The same doc-type rule as for method return types.
pub(crate) fn enrich_property_from_ancestor(existing: &mut PropertyInfo, ancestor: &PropertyInfo) {
    // Asked before the type below copies the ancestor's doc type.
    let takes_description = inherits_property_description(existing, ancestor);

    // ── Type hint ───────────────────────────────────────────────
    if inherits_property_type(existing, ancestor) {
        existing.type_hint = ancestor.type_hint.clone();
        existing.doc_type = ancestor.doc_type.clone();
    }

    // ── Description ─────────────────────────────────────────────
    if takes_description {
        existing.description = ancestor.description.clone();
    }
}
