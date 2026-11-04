//! The route grid of Plan 06 session 1 (extraction keeps every fact): each
//! rule extraction owns, asked through every route that reaches it, with
//! one decided answer.
//!
//! - Row A: which docblock belongs to a declaration next to attributes
//!   (issue 17 + N1, ATTR-001 to ATTR-004).
//!
//! Intended differences between routes: hover shows types and texts, a
//! completion `detail` shows the type only, signature help shows the
//! parameter part, diagnostics show no data. A parsed record holds only
//! what its declaration wrote; a resolved one adds what inheritance
//! supplies.

use crate::common::{
    assert_var_type, assert_var_type_on, collect_diagnostics_with, completion_response,
    create_psr4_workspace, create_test_backend, hover_markdown_on, messages_with_code,
    offset_position, open_php,
};
use phpantom_lsp::php_type::PhpType;
use phpantom_lsp::types::{MethodInfo, ParameterInfo, PropertyInfo};
use phpantom_lsp::{Backend, ClassInfo};
use tower_lsp_server::LanguageServer;
use tower_lsp_server::gen_lsp_types::*;

// ─── Records ────────────────────────────────────────────────────────────────

/// The class `name` among `classes`.
fn class<'a>(classes: &'a [ClassInfo], name: &str) -> &'a ClassInfo {
    classes
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no class {name}"))
}

/// The property `name` of `class`.
fn property<'a>(class: &'a ClassInfo, name: &str) -> &'a PropertyInfo {
    class
        .properties
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("no property {}::${name}", class.name))
}

/// The method `name` of `class`.
fn method<'a>(class: &'a ClassInfo, name: &str) -> &'a MethodInfo {
    class
        .methods
        .iter()
        .find(|m| m.name == name)
        .unwrap_or_else(|| panic!("no method {}::{name}", class.name))
}

/// The parameter `name` (with its `$`) of `method`.
fn param<'a>(method: &'a MethodInfo, name: &str) -> &'a ParameterInfo {
    method
        .parameters
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("no parameter {name} on {}", method.name))
}

/// A stored type as text, `None` when absent.
fn text(ty: &Option<PhpType>) -> Option<String> {
    ty.as_ref().map(ToString::to_string)
}

// ─── Completion and signature help ──────────────────────────────────────────

/// The completion items offered right after the first occurrence of
/// `after` in `content`, opened as `uri`.
async fn completion_items(
    backend: &Backend,
    uri: &Uri,
    content: &str,
    after: &str,
) -> Vec<CompletionItem> {
    open_php(backend, uri, content).await;
    let offset = content
        .find(after)
        .unwrap_or_else(|| panic!("{after:?} not found"))
        + after.len();
    let position = offset_position(content, offset);
    match completion_response(backend, uri, position.line, position.character, None).await {
        Some(CompletionResponse::CompletionItemList(items)) => items,
        Some(CompletionResponse::CompletionList(list)) => list.items,
        None => panic!("no completion after {after:?}"),
    }
}

/// The `detail` of the completion item `label` offered right after the
/// first occurrence of `after` in `content`, opened as `uri`.
async fn completion_detail(
    backend: &Backend,
    uri: &Uri,
    content: &str,
    after: &str,
    label: &str,
) -> Option<String> {
    completion_items(backend, uri, content, after)
        .await
        .into_iter()
        .find(|item| item.label == label)
        .unwrap_or_else(|| panic!("no `{label}` completion after {after:?}"))
        .detail
}

/// One parameter as signature help shows it: the type it reports (the
/// effective type in the documentation's code span when it differs from
/// the native one, otherwise the native type of the label) with the
/// name.
#[derive(Debug)]
struct ShownParam {
    typed: String,
}

/// The parameters signature help shows right after the first occurrence
/// of `call` (which ends with its `(`) in `content`, opened as `uri`.
async fn signature_params(
    backend: &Backend,
    uri: &Uri,
    content: &str,
    call: &str,
) -> Vec<ShownParam> {
    open_php(backend, uri, content).await;
    let offset = content
        .find(call)
        .unwrap_or_else(|| panic!("{call:?} not found"))
        + call.len();
    let position = offset_position(content, offset);
    let help = backend
        .signature_help(SignatureHelpParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            context: None,
        })
        .await
        .unwrap()
        .unwrap_or_else(|| panic!("no signature help after {call:?}"));
    let signature = &help.signatures[0];
    signature
        .parameters
        .as_ref()
        .expect("parameters")
        .iter()
        .map(|parameter| {
            let label = match &parameter.label {
                ParameterInformationLabel::String(label) => label.clone(),
                ParameterInformationLabel::Tuple((start, end)) => {
                    signature.label[*start as usize..*end as usize].to_string()
                }
            };
            let (native, name) = label
                .rsplit_once(' ')
                .map_or((None, label.as_str()), |(ty, name)| (Some(ty), name));
            let doc = parameter.documentation.as_ref().map(|doc| match doc {
                Documentation::String(text) => text.clone(),
                Documentation::MarkupContent(markup) => markup.value.clone(),
            });
            let code_span = doc.as_deref().and_then(|doc| {
                let rest = doc.strip_prefix('`')?;
                Some(rest.split_once('`')?.0.to_string())
            });
            let ty = code_span.or_else(|| native.map(str::to_string));
            ShownParam {
                typed: match ty {
                    Some(ty) => format!("{ty} {name}"),
                    None => name.to_string(),
                },
            }
        })
        .collect()
}

/// The shown types of `params`, for assertions that ignore the texts.
fn typed(params: &[ShownParam]) -> Vec<&str> {
    params.iter().map(|p| p.typed.as_str()).collect()
}

// ─── Row A: docblock association (issue 17 + N1) ────────────────────────────

/// B1: promoted constructor parameters with a docblock after (`$secret`)
/// and before (`$pin`) the attribute.
const B1: &str =
    include_str!("../conformance/php-symbol-metadata/attributes/promoted-after-attribute.php");

/// B2: a property and a method with the docblock after the attribute,
/// and a property with it before.
const B2: &str =
    include_str!("../conformance/php-symbol-metadata/attributes/members-after-attribute.php");

/// A1: the two places the type engine reads docblocks itself (session 1
/// report, section 7.1): a promoted parameter in the constructor body, and
/// a method template bound at a call.
const A1: &str = r#"<?php
#[Attribute] class Tag {}
class Vault {}
class Keeper {
    public function __construct(
        #[Tag]
        /** @var non-empty-string */
        public string $secret,
    ) { $__a1 = $secret; }
    #[Tag]
    /**
     * @template T of object
     * @param T $item
     * @return T
     */
    public function keep(object $item): object { return $item; }
}
$__a2 = (new Keeper('x'))->keep(new Vault());
"#;

/// The records hold the docblock written after the attribute as well as
/// the one written before it.
///
/// Spec: ATTR-001, ATTR-002.
#[test]
fn issue_17_docblock_after_attribute_record() {
    let backend = create_test_backend();

    let classes = backend.parse_php(B1);
    let vault = class(&classes, "Vault");
    let constructor = method(vault, "__construct");
    for (name, doc, description) in [
        ("secret", "non-empty-string", "Own text."),
        ("pin", "positive-int", "Before text."),
    ] {
        let prop = property(vault, name);
        assert_eq!(text(&prop.doc_type).as_deref(), Some(doc), "${name}");
        assert_eq!(prop.description.as_deref(), Some(description), "${name}");
        let parameter = param(constructor, &format!("${name}"));
        assert_eq!(
            text(&parameter.doc_type).as_deref(),
            Some(doc),
            "parameter ${name}"
        );
    }

    let classes = backend.parse_php(B2);
    let boxed = class(&classes, "Box");
    for (name, description) in [("items", "Own items text."), ("control", "Control text.")] {
        let prop = property(boxed, name);
        assert_eq!(
            text(&prop.doc_type).as_deref(),
            Some("list<string>"),
            "${name}"
        );
        assert_eq!(prop.description.as_deref(), Some(description), "${name}");
    }
    let names = method(boxed, "names");
    assert_eq!(
        text(&names.doc_return_type).as_deref(),
        Some("list<string>")
    );
    assert_eq!(names.return_description(), Some("Own names text."));
}

/// The declaration hover shows the type and text of a docblock written
/// after the attribute, as the hover on a use does.
///
/// Spec: ATTR-002.
#[test]
fn issue_17_docblock_after_attribute_declaration_hover() {
    let backend = create_test_backend().with_hover_on_declarations(true);
    for (content, needle, delta, present) in [
        (
            B1,
            "public string $secret",
            15,
            ["non-empty-string", "Own text."],
        ),
        (
            B2,
            "function names",
            10,
            ["list<string>", "Own names text."],
        ),
    ] {
        let markdown = hover_markdown_on(&backend, "file:///test.php", content, needle, delta, 0)
            .unwrap_or_else(|| panic!("expected a declaration hover at {needle:?}"));
        for expected in present {
            assert!(
                markdown.contains(expected),
                "{needle:?}: expected {expected:?} in:\n{markdown}"
            );
        }
    }
}

/// The type engine types a member, a promoted parameter and a template
/// call from a docblock written after the attribute.
///
/// Spec: ATTR-002, D-02.
#[test]
fn issue_17_docblock_after_attribute_type_engine() {
    assert_var_type(
        &format!("{B1}$__r1 = $v->secret;\n"),
        "$__r1",
        "non-empty-string",
    );
    assert_var_type(B2, "$n", "list<string>");
    assert_var_type(A1, "$__a1", "non-empty-string");
    assert_var_type(A1, "$__a2", "Vault");
}

/// Completion details carry the type of a docblock written after the
/// attribute.
///
/// Spec: ATTR-002.
#[tokio::test]
async fn issue_17_docblock_after_attribute_completion() {
    let uri = Uri::parse("file:///test.php").unwrap();
    for (content, after, label, detail) in [
        (B1, "echo $v->", "secret", "non-empty-string"),
        (B2, "echo $b->", "items", "list<string>"),
    ] {
        let backend = create_test_backend();
        assert_eq!(
            completion_detail(&backend, &uri, content, after, label)
                .await
                .as_deref(),
            Some(detail),
            "{label}"
        );
    }
}

/// Signature help of the constructor shows the types of the promoted
/// parameters' docblocks, before and after the attribute alike. Types
/// only: whether a promoted `@var` text is also a parameter text is not
/// decided.
///
/// Spec: ATTR-001, ATTR-002.
#[tokio::test]
async fn issue_17_docblock_after_attribute_signature_help() {
    let backend = create_test_backend();
    let uri = Uri::parse("file:///test.php").unwrap();
    let params = signature_params(&backend, &uri, B1, "new Vault(").await;
    assert_eq!(
        typed(&params),
        ["non-empty-string $secret", "positive-int $pin"]
    );
}

/// The docblock/native mismatch warning appears with the same message
/// whether the docblock is written before or after the attribute; the
/// row's sources get none.
///
/// Spec: ATTR-001, ATTR-002.
#[test]
fn issue_17_docblock_after_attribute_diagnostics() {
    const CODE: &str = "docblock_native_mismatch";
    let warnings = |php: &str| {
        let diags = collect_diagnostics_with(
            &create_test_backend(),
            php,
            Backend::collect_docblock_native_mismatch_diagnostics,
        );
        messages_with_code(&diags, CODE)
    };
    let before = warnings(
        "<?php\n#[Attribute] class Tag {}\n/** @param ?string $name */\n#[Tag]\nfunction greet(string $name): void {}\n",
    );
    let after = warnings(
        "<?php\n#[Attribute] class Tag {}\n#[Tag]\n/** @param ?string $name */\nfunction greet(string $name): void {}\n",
    );
    assert_eq!(before.len(), 1, "docblock before the attribute: {before:?}");
    assert_eq!(after, before, "docblock after the attribute");
    for (name, source) in [("B1", B1), ("B2", B2)] {
        assert_eq!(warnings(source), Vec::<String>::new(), "{name}");
    }
}

/// A class loaded from a PSR-4 file that is not open reads the docblock
/// written after the attribute too.
///
/// Spec: ATTR-002.
#[test]
fn issue_17_docblock_after_attribute_from_a_file_not_open() {
    let vault = B1.replacen("<?php\n", "<?php\nnamespace App;\n", 1);
    let (backend, dir) = create_psr4_workspace(
        r#"{"autoload": {"psr-4": {"App\\": "src/"}}}"#,
        &[("src/Vault.php", &vault)],
    );
    let main = "<?php\n$v = new \\App\\Vault('x', 1);\n$__r1 = $v->secret;\n";
    let uri = Uri::from_file_path(dir.path().join("main.php")).unwrap();
    assert_var_type_on(&backend, uri.as_str(), main, "$__r1", "non-empty-string");
}
