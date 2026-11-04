//! The route grid of Plan 06 session 1 (extraction keeps every fact): each
//! rule extraction owns, asked through every route that reaches it, with
//! one decided answer.
//!
//! - Row A: which docblock belongs to a declaration next to attributes
//!   (issue 17 + N1, ATTR-001 to ATTR-004).
//! - Row B: the written doc type, and inheritance read from it (issues 23
//!   and 27, D-03, D-04, D-06).
//! - Row C: one `define()` recogniser (issues 6 and 8, D-13, D-14).
//!
//! Intended differences between routes: hover shows types and texts, a
//! completion `detail` shows the type only, signature help shows the
//! parameter part, diagnostics show no data. A parsed record holds only
//! what its declaration wrote; a resolved one adds what inheritance
//! supplies.

use std::sync::Arc;

use crate::common::{
    assert_var_type, assert_var_type_on, collect_diagnostics_with, completion_response,
    create_psr4_workspace, create_test_backend, create_test_backend_with_full_stubs,
    definition_locations, goto_definition_at, hover_markdown, hover_markdown_on,
    messages_with_code, offset_position, open_php, response_labels,
};
use phpantom_lsp::php_type::PhpType;
use phpantom_lsp::types::{MethodInfo, ParameterInfo, PropertyInfo};
use phpantom_lsp::{Backend, ClassInfo, Fqn, resolve_class_fully};
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
/// name, and the parameter's text.
#[derive(Debug)]
struct ShownParam {
    typed: String,
    text: Option<String>,
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
                let (ty, after) = rest.split_once('`')?;
                Some((ty.to_string(), after.trim().to_string()))
            });
            let (ty, text) = match code_span {
                Some((ty, after)) => (Some(ty), Some(after).filter(|t| !t.is_empty())),
                None => (native.map(str::to_string), doc),
            };
            ShownParam {
                typed: match ty {
                    Some(ty) => format!("{ty} {name}"),
                    None => name.to_string(),
                },
                text,
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

// ─── Row B: the written doc type, and inheritance read from it ──────────────

/// D3: a parent with narrower doc types, and overrides that document the
/// same types explicitly, document nothing, or document only a summary.
const D3: &str = include_str!("../conformance/php-symbol-metadata/inheritance/partial-docs.php");

/// Q1: an undocumented override of a stub method whose type a stub patch
/// supplies. PHPStan 2.3.1 types both calls `Foo`.
const Q1: &str = r#"<?php
class Foo {}
/** @extends IteratorIterator<int, Foo, ArrayIterator<int, Foo>> */
class MyIt extends IteratorIterator {
    public function current(): mixed { return parent::current(); }
}
/** @extends IteratorIterator<int, Foo, ArrayIterator<int, Foo>> */
class PlainIt extends IteratorIterator {}
function f(MyIt $it, PlainIt $p): void {
    $__d1 = $it->current();
    $__d2 = $p->current();
}
"#;

/// Q2: a `@param` without a type, with and without text. PHPStan 2.3.1
/// types both `$id` as the parent's `int<1, max>`; a tag without a type
/// counts as absent unless it has text.
const Q2: &str = r#"<?php
class P {
    /** @param positive-int $id Parent text. */
    public function find(int $id): void {}
}
class C extends P {
    /** @param $id */
    public function find(int $id): void { $__d1 = $id; }
}
class D extends P {
    /** @param $id Child text only. */
    public function find(int $id): void { $__d2 = $id; }
}
"#;

/// Q3: a promoted property typed by the constructor's `@phpstan-param`
/// (PHPStan 2.3.1: `list<int>`).
const Q3: &str = r#"<?php
class Inventory {
    /**
     * @param array $items The items.
     * @phpstan-param list<int> $items
     */
    public function __construct(public array $items) {}
}
$__d1 = (new Inventory([]))->items;
"#;

/// Q4: the nearest documented ancestor wins, even when its doc type equals
/// the native type (PHPStan 2.3.1: `int` and `string`).
const Q4: &str = r#"<?php
class P {
    /**
     * @param positive-int $id
     * @return non-empty-string
     */
    public function find(int $id): string { return 'x'; }
}
class ExplicitEqual extends P {
    /**
     * @param int $id
     * @return string
     */
    public function find(int $id): string { return ''; }
}
class Leaf extends ExplicitEqual {
    public function find(int $id): string { $__d1 = $id; return ''; }
}
$__d2 = (new Leaf())->find(1);
"#;

/// L1 (issue 27): an undocumented override of an Eloquent relationship
/// whose related model the parent's body supplies. The override answers
/// its parent's type, `HasMany<Post>`, on every route.
const L1: &str = r#"<?php
namespace App\Models;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\HasMany;
class Post extends Model {}
class Author extends Model {
    public function posts(): HasMany { return $this->hasMany(Post::class); }
}
class GuestAuthor extends Author {
    public function posts(): HasMany { return parent::posts(); }
}
$__l1 = (new Author())->posts();
$__l2 = (new GuestAuthor())->posts();
"#;

/// The part of the Laravel `Model` stub of the Laravel integration tests
/// that L1 calls, with the classes its types name.
const L1_FRAMEWORK: &[(&str, &str)] = &[
    (
        "vendor/illuminate/Eloquent/Model.php",
        r#"<?php
namespace Illuminate\Database\Eloquent;
use Illuminate\Database\Eloquent\Relations\HasMany;
class Model {
    /**
     * @template TRelatedModel of Model
     * @param class-string<TRelatedModel> $related
     * @return HasMany<TRelatedModel, $this>
     */
    public function hasMany($related, $foreignKey = null, $localKey = null) {}
}
"#,
    ),
    (
        "vendor/illuminate/Eloquent/Collection.php",
        "<?php\nnamespace Illuminate\\Database\\Eloquent;\n/**\n * @template TKey of array-key\n * @template TModel\n */\nclass Collection {}\n",
    ),
    (
        "vendor/illuminate/Eloquent/Relations/HasMany.php",
        "<?php\nnamespace Illuminate\\Database\\Eloquent\\Relations;\nclass HasMany {}\n",
    ),
];

const L1_COMPOSER: &str = r#"{"autoload": {"psr-4": {
    "App\\Models\\": "src/Models/",
    "Illuminate\\Database\\Eloquent\\": "vendor/illuminate/Eloquent/",
    "Illuminate\\Database\\Eloquent\\Relations\\": "vendor/illuminate/Eloquent/Relations/"
}}}"#;

/// A workspace with the L1 framework files, and the URI L1 is parsed as
/// (`src/Models/Authors.php`, never written to disk).
fn l1_workspace() -> (Backend, tempfile::TempDir, String) {
    let (backend, dir) = create_psr4_workspace(L1_COMPOSER, L1_FRAMEWORK);
    let uri = Uri::from_file_path(dir.path().join("src/Models/Authors.php"))
        .unwrap()
        .as_str()
        .to_string();
    (backend, dir, uri)
}

/// `ty` with every class name shortened to its last segment, the way
/// hovers print types.
fn short(ty: &str) -> String {
    let mut out = String::new();
    let mut word = String::new();
    for c in ty.chars().chain(['\0']) {
        if c.is_alphanumeric() || c == '_' || c == '\\' {
            word.push(c);
            continue;
        }
        out.push_str(word.rsplit('\\').next().unwrap_or_default());
        word.clear();
        if c != '\0' {
            out.push(c);
        }
    }
    out
}

/// `classes`, loadable by name, the way the index would serve them.
fn loader_of(classes: Vec<ClassInfo>) -> impl Fn(Fqn<'_>) -> Option<Arc<ClassInfo>> {
    let classes: Vec<Arc<ClassInfo>> = classes.into_iter().map(Arc::new).collect();
    move |name: Fqn<'_>| {
        let name = name.as_str();
        let name = name.rsplit('\\').next().unwrap_or(name);
        classes.iter().find(|c| c.name == name).cloned()
    }
}

/// Each declaration's own record holds the doc type it wrote, and none
/// when it wrote none, also when the written type equals the native one.
///
/// Spec: INH-004; decisions D-01, D-03.
#[test]
fn issue_23_written_doc_type_local_record() {
    let backend = create_test_backend();

    let classes = backend.parse_php(D3);
    let p = class(&classes, "P");
    let find = method(p, "find");
    assert_eq!(
        text(&param(find, "$id").doc_type).as_deref(),
        Some("positive-int")
    );
    assert_eq!(
        text(&find.doc_return_type).as_deref(),
        Some("non-empty-string")
    );
    assert_eq!(
        text(&property(p, "label").doc_type).as_deref(),
        Some("non-empty-string")
    );

    let explicit = class(&classes, "ExplicitEqual");
    let find = method(explicit, "find");
    assert_eq!(text(&param(find, "$id").doc_type).as_deref(), Some("int"));
    assert_eq!(text(&find.doc_return_type).as_deref(), Some("string"));
    assert_eq!(
        text(&property(explicit, "label").doc_type).as_deref(),
        Some("string")
    );

    for name in ["NoDoc", "DescriptionOnly"] {
        let find = method(class(&classes, name), "find");
        for parameter in ["$id", "$name"] {
            assert_eq!(param(find, parameter).doc_type, None, "{name} {parameter}");
        }
        assert_eq!(find.doc_return_type, None, "{name} return");
    }
    assert_eq!(property(class(&classes, "NoDoc"), "label").doc_type, None);

    let classes = backend.parse_php(Q2);
    for name in ["C", "D"] {
        let find = method(class(&classes, name), "find");
        assert_eq!(param(find, "$id").doc_type, None, "{name} $id");
    }

    let classes = backend.parse_php(Q3);
    assert_eq!(
        text(&property(class(&classes, "Inventory"), "items").doc_type).as_deref(),
        Some("list<int>")
    );
}

/// A resolved record takes its doc types from the nearest documented
/// ancestor, through undocumented ones, and resolving a child leaves the
/// parent's parsed record as it was.
///
/// Spec: INH-001, INH-010, INH-014; decisions D-03, D-06.
#[test]
fn issue_23_written_doc_type_resolved_record() {
    let backend = create_test_backend();

    let classes = backend.parse_php(D3);
    let parent_before = format!("{:?}", class(&classes, "P"));
    let no_doc = class(&classes, "NoDoc").clone();
    let grand = class(&classes, "Grand").clone();
    let loader = loader_of(classes);
    for child in [&no_doc, &grand] {
        let resolved = resolve_class_fully(child, &loader);
        let find = method(&resolved, "find");
        let id = param(find, "$id");
        assert_eq!(
            text(&id.doc_type).as_deref(),
            Some("positive-int"),
            "{}",
            child.name
        );
        assert_eq!(
            text(&id.type_hint).as_deref(),
            Some("positive-int"),
            "{}",
            child.name
        );
        assert_eq!(
            text(&find.doc_return_type).as_deref(),
            Some("non-empty-string"),
            "{}",
            child.name
        );
        assert_eq!(
            text(&find.return_type).as_deref(),
            Some("non-empty-string"),
            "{}",
            child.name
        );
    }
    let resolved = resolve_class_fully(&no_doc, &loader);
    let label = property(&resolved, "label");
    assert_eq!(text(&label.doc_type).as_deref(), Some("non-empty-string"));
    assert_eq!(text(&label.type_hint).as_deref(), Some("non-empty-string"));
    let parent_after = loader(Fqn::new("P")).expect("P");
    assert_eq!(
        format!("{parent_after:?}"),
        parent_before,
        "P's parsed record"
    );

    let classes = backend.parse_php(Q4);
    let leaf = class(&classes, "Leaf").clone();
    let resolved = resolve_class_fully(&leaf, &loader_of(classes));
    let find = method(&resolved, "find");
    let id = param(find, "$id");
    assert_eq!(text(&id.doc_type).as_deref(), Some("int"));
    assert_eq!(text(&id.type_hint).as_deref(), Some("int"));
    assert_eq!(text(&find.doc_return_type).as_deref(), Some("string"));
    assert_eq!(text(&find.return_type).as_deref(), Some("string"));
}

/// A member hover shows the types and texts the record decided.
///
/// Spec: INH-001, INH-004; decisions D-01, D-03, D-10.
#[test]
fn issue_23_written_doc_type_hover() {
    let q2 = format!("{Q2}(new C())->find(1);\n(new D())->find(1);\n");
    // Each needle ends with the member name; the hover is asked on its
    // first character.
    let cases: [(&str, &str, &[&str], &[&str]); 4] = [
        (&q2, "(new C())->find", &["Parent text."], &[]),
        (
            &q2,
            "(new D())->find",
            &["Child text only."],
            &["Parent text."],
        ),
        (
            Q4,
            "(new Leaf())->find",
            &["**$id** `int`"],
            &["positive-int"],
        ),
        (
            Q3,
            "(new Inventory([]))->items",
            &["`list<int>`", "The items."],
            &[],
        ),
    ];
    for (content, needle, present, absent) in cases {
        let name_start = needle.rfind("->").unwrap() as u32 + 2;
        let markdown = hover_markdown(content, needle, name_start, 0)
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

    let backend = create_test_backend_with_full_stubs();
    let markdown = hover_markdown_on(&backend, "file:///test.php", Q1, "$it->current()", 6, 0)
        .expect("expected a hover on ->current()");
    assert!(markdown.contains("Foo"), "Q1 ->current(): {markdown}");
}

/// The type engine reads the decided types in the bodies and at the
/// calls.
///
/// Spec: INH-001, INH-004; decisions D-01, D-03.
#[test]
fn issue_23_written_doc_type_type_engine() {
    assert_var_type(Q2, "$__d1", "positive-int");
    assert_var_type(Q2, "$__d2", "positive-int");
    assert_var_type(Q4, "$__d1", "int");
    assert_var_type(Q4, "$__d2", "string");
    assert_var_type(Q3, "$__d1", "list<int>");
    assert_var_type_on(
        &create_test_backend_with_full_stubs(),
        "file:///test.php",
        Q1,
        "$__d1",
        "Foo",
    );
}

/// Completion details carry the decided property types.
///
/// Spec: INH-014, PROP-004; decisions D-01, D-06.
#[tokio::test]
async fn issue_23_written_doc_type_completion() {
    let uri = Uri::parse("file:///test.php").unwrap();
    for (content, after, label, detail) in [
        (D3, "(new ExplicitEqual())->", "label", "string"),
        (D3, "(new NoDoc())->", "label", "non-empty-string"),
        (Q3, "(new Inventory([]))->", "items", "list<int>"),
    ] {
        let backend = create_test_backend();
        assert_eq!(
            completion_detail(&backend, &uri, content, after, label)
                .await
                .as_deref(),
            Some(detail),
            "{label} after {after:?}"
        );
    }
}

/// Signature help shows the decided parameter types and texts.
///
/// Spec: INH-001, INH-004, INH-006; decisions D-01, D-03, D-04, D-10.
#[tokio::test]
async fn issue_23_written_doc_type_signature_help() {
    let uri = Uri::parse("file:///test.php").unwrap();
    for (content, call, expected) in [
        (
            D3,
            "(new NoDoc())->find(",
            &["positive-int $id", "non-empty-string $name"][..],
        ),
        (
            D3,
            "(new ExplicitEqual())->find(",
            &["int $id", "non-empty-string $name"][..],
        ),
        (Q4, "(new Leaf())->find(", &["int $id"][..]),
        (Q3, "new Inventory(", &["list<int> $items"][..]),
    ] {
        let backend = create_test_backend();
        let params = signature_params(&backend, &uri, content, call).await;
        assert_eq!(typed(&params), expected, "{call:?}");
    }

    let q2 = format!("{Q2}(new C())->find(1);\n(new D())->find(1);\n");
    for (call, text) in [
        ("(new C())->find(", "Parent text."),
        ("(new D())->find(", "Child text only."),
    ] {
        let backend = create_test_backend();
        let params = signature_params(&backend, &uri, &q2, call).await;
        assert_eq!(params[0].text.as_deref(), Some(text), "{call:?}");
    }
}

/// Classes loaded from PSR-4 files that are not open answer the same
/// call types.
///
/// Spec: INH-001, INH-004; decision D-03.
#[test]
fn issue_23_written_doc_type_from_a_file_not_open() {
    let files = [
        (
            "src/P.php",
            "<?php\nnamespace App;\nclass P {\n    /**\n     * @param positive-int $id\n     * @param non-empty-string $name\n     * @return non-empty-string\n     */\n    public function find(int $id, string $name): string { return 'x'; }\n}\n",
        ),
        (
            "src/ExplicitEqual.php",
            "<?php\nnamespace App;\nclass ExplicitEqual extends P {\n    /**\n     * @param int $id\n     * @return string\n     */\n    public function find(int $id, string $name): string { return ''; }\n}\n",
        ),
        (
            "src/NoDoc.php",
            "<?php\nnamespace App;\nclass NoDoc extends P {\n    public function find(int $id, string $name): string { return 'x'; }\n}\n",
        ),
        (
            "src/Leaf.php",
            "<?php\nnamespace App;\nclass Leaf extends ExplicitEqual {\n    public function find(int $id, string $name): string { return ''; }\n}\n",
        ),
    ];
    let (backend, dir) =
        create_psr4_workspace(r#"{"autoload": {"psr-4": {"App\\": "src/"}}}"#, &files);
    let main = "<?php\n$__f1 = (new \\App\\NoDoc())->find(1, 'a');\n$__f2 = (new \\App\\ExplicitEqual())->find(1, 'a');\n$__f3 = (new \\App\\Leaf())->find(1, 'a');\n";
    let uri = Uri::from_file_path(dir.path().join("main.php")).unwrap();
    for (var, expected) in [
        ("$__f1", "non-empty-string"),
        ("$__f2", "string"),
        ("$__f3", "string"),
    ] {
        assert_var_type_on(&backend, uri.as_str(), main, var, expected);
    }
}

/// The `**var**` type a property hover shows.
fn property_hover_type(markdown: &str) -> Option<String> {
    markdown
        .lines()
        .find_map(|line| line.strip_prefix("**var** `")?.strip_suffix('`'))
        .map(str::to_string)
}

/// L1 with a read of the relationship property on each class appended.
fn l1_with_property_reads() -> String {
    format!("{L1}(new Author())->posts;\n(new GuestAuthor())->posts;\n")
}

/// The relationship method's record holds the type its body inferred as
/// a doc return type, and the undocumented override inherits it.
#[test]
#[ignore = "issue 27: the body-inferred relationship type is not recorded as a doc return type, so the override keeps a bare HasMany"]
fn issue_27_relationship_override_record() {
    let backend = create_test_backend();
    let classes = backend.parse_php(L1);
    let posts = method(class(&classes, "Author"), "posts");
    assert_eq!(
        text(&posts.doc_return_type).map(|t| short(&t)).as_deref(),
        Some("HasMany<Post>")
    );
    let guest = class(&classes, "GuestAuthor").clone();
    let resolved = resolve_class_fully(&guest, &loader_of(classes));
    let posts = method(&resolved, "posts");
    assert_eq!(
        text(&posts.return_type).map(|t| short(&t)).as_deref(),
        Some("HasMany<Post>")
    );
}

/// The hover on the override's call shows the parent's relationship type,
/// and the relationship property has the same type on both classes.
#[test]
#[ignore = "issue 27: the override of a relationship method hovers as a bare HasMany"]
fn issue_27_relationship_override_hover() {
    let (backend, _dir, uri) = l1_workspace();
    let content = l1_with_property_reads();
    let markdown = hover_markdown_on(
        &backend,
        &uri,
        &content,
        "(new GuestAuthor())->posts()",
        21,
        0,
    )
    .expect("expected a hover on the override's call");
    assert!(
        markdown.contains("HasMany<Post>"),
        "override call: {markdown}"
    );
    let property_type = |needle: &str| {
        let name_start = needle.rfind("->").unwrap() as u32 + 2;
        let markdown = hover_markdown_on(&backend, &uri, &content, needle, name_start, 0)
            .unwrap_or_else(|| panic!("expected a hover at {needle:?}"));
        property_hover_type(&markdown)
    };
    let on_author = property_type("(new Author())->posts;");
    assert!(on_author.is_some(), "no type on Author's posts property");
    assert_eq!(property_type("(new GuestAuthor())->posts;"), on_author);
}

/// The type engine gives the call through the override the parent's
/// relationship type.
#[test]
#[ignore = "issue 27: a call through the override of a relationship method answers a bare HasMany"]
fn issue_27_relationship_override_type_engine() {
    let (backend, _dir, uri) = l1_workspace();
    for var in ["$__l1", "$__l2"] {
        assert_var_type_on(&backend, &uri, L1, var, "HasMany<Post>");
    }
}

/// Completion offers `posts` with the same details on both classes.
#[tokio::test]
#[ignore = "issue 27: completion details of posts differ between the relationship and its override"]
async fn issue_27_relationship_override_completion() {
    let content = l1_with_property_reads();
    let mut details = Vec::new();
    for after in ["(new Author())->", "(new GuestAuthor())->"] {
        let (backend, _dir, uri) = l1_workspace();
        let uri = Uri::parse(&uri).unwrap();
        let mut posts: Vec<(String, Option<String>)> =
            completion_items(&backend, &uri, &content, after)
                .await
                .into_iter()
                .filter(|item| item.label.starts_with("posts"))
                .map(|item| (item.label, item.detail))
                .collect();
        posts.sort();
        assert!(!posts.is_empty(), "no posts completion after {after:?}");
        details.push(posts);
    }
    assert_eq!(details[1], details[0]);
}

// ─── Row C: one define() recogniser (issues 6, 8) ───────────────────────────

/// C3 (copied from `hover_metadata_constants.rs`): `define('\LIMIT', …)`
/// declares a constant that no unqualified `LIMIT` reaches (D-13).
const C3: &str = r#"<?php
define('\\LIMIT', 1);
echo LIMIT;
"#;

/// C4 (copied): `defined('X') or define('X', …)` (issue 6).
const C4: &str = r#"<?php
defined('ORX') or define('ORX', 5);
echo ORX;
"#;

/// C5 (copied): a fully qualified `\define()` inside a namespace (issue 8).
const C5: &str = r#"<?php
namespace App;
\define('GLOBAL_NAME', 20);
echo \GLOBAL_NAME;
"#;

/// A backend with `content` open as `file:///main.php`.
async fn open_main(content: &str) -> (Backend, Uri) {
    let backend = create_test_backend();
    let uri = Uri::parse("file:///main.php").unwrap();
    open_php(&backend, &uri, content).await;
    (backend, uri)
}

/// A constant read from either `define()` form has its value's type.
///
/// Decision D-14.
#[test]
fn issue_6_8_define_forms_type_engine() {
    assert_var_type(&format!("{C4}$__c = ORX;\n"), "$__c", "5");
    assert_var_type(&format!("{C5}$__g = \\GLOBAL_NAME;\n"), "$__g", "20");
}

/// Completion offers the constants either form declares, by name.
///
/// Decision D-14.
#[tokio::test]
async fn issue_6_8_define_forms_completion() {
    for (content, after, label) in [
        (C4, "echo OR", "ORX"),
        (C5, "echo \\GLOBAL_", "GLOBAL_NAME"),
    ] {
        let (backend, uri) = open_main(content).await;
        let offset = content.find(after).unwrap() + after.len();
        let position = offset_position(content, offset);
        let labels = response_labels(
            completion_response(&backend, &uri, position.line, position.character, None).await,
        );
        assert!(
            labels.iter().any(|l| l == label),
            "{label} after {after:?}: {labels:?}"
        );
    }
}

/// Go-to-definition on a constant either form declares lands on the line
/// of its `define()` call; `define('\LIMIT', …)` declares nothing an
/// unqualified `LIMIT` reaches.
///
/// Decisions D-13, D-14.
#[tokio::test]
async fn issue_6_8_define_forms_definition() {
    for (content, needle, delta, line) in [
        (C4, "echo ORX", 6, Some(1)),
        (C5, "echo \\GLOBAL_NAME", 7, Some(2)),
        (C3, "echo LIMIT", 6, None),
    ] {
        let (backend, uri) = open_main(content).await;
        let offset = content.find(needle).unwrap() + delta;
        let position = offset_position(content, offset);
        let found: Vec<(String, u32)> = definition_locations(
            goto_definition_at(&backend, &uri, position.line, position.character).await,
        )
        .into_iter()
        .map(|location| (location.uri.as_str().to_string(), location.range.start.line))
        .collect();
        let expected: Vec<(String, u32)> = line
            .map(|line| ("file:///main.php".to_string(), line))
            .into_iter()
            .collect();
        assert_eq!(found, expected, "{needle:?}");
    }
}
