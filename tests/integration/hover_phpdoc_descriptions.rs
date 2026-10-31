//! Descriptions of magic members reach hover through the shared member data.

use crate::common::{create_test_backend, hover_at, hover_last, hover_text, offset_position};

#[test]
fn hover_phpdoc_member_descriptions() {
    let backend = create_test_backend();
    let content = r#"<?php
/**
 * @property string $label The visible label.
 * @property-read int $views How often it was viewed.
 * @property-write string $title The title to save.
 * @method string render(int $width) Renders the post.
 * @method total(): int Sums the lines.
 * @method static string heading() Builds the heading.
 */
class Post {}
$post = new Post();
echo $post->label;
echo $post->views;
$post->title = 'Saved';
echo $post->render(80);
echo $post->total();
echo Post::heading();
"#;
    for (needle, description) in [
        ("label;", "The visible label."),
        ("views;", "How often it was viewed."),
        ("title =", "The title to save."),
        ("render(80)", "Renders the post."),
        ("total();", "Sums the lines."),
        ("heading();", "Builds the heading."),
    ] {
        let text = hover_last(&backend, content, needle);
        assert!(text.contains(description), "{needle}: {text}");
        assert!(text.contains("virtual"), "{needle}: {text}");
        assert!(!text.contains(": int Sums"), "{text}");
    }
}

#[test]
fn hover_phpdoc_descriptions_survive_inheritance_mixins_and_templates() {
    let backend = create_test_backend();
    let content = r#"<?php
namespace Library {
    class Item {}
    /**
     * @template T
     * @property T $value The wrapped item.
     * @property Item $fallback The fallback item.
     * @method T unwrap() Unwraps the item.
     * @method Item fallback() Returns the fallback item.
     */
    class Box {}
}
namespace App {
    use Library\Box;
    use Library\Item;
    /** @extends Box<Item> */
    class ItemBox extends Box {}
    /** @mixin Box<Item> */
    class BoxProxy {}
    $box = new ItemBox();
    echo $box->value;
    $box->unwrap();
    echo $box->fallback;
    $box->fallback();
    $proxy = new BoxProxy();
    echo $proxy->value;
    $proxy->unwrap();
    echo $proxy->fallback;
    $proxy->fallback();
}
"#;
    for (receiver, member, description) in [
        ("box", "value;", "The wrapped item."),
        ("box", "unwrap();", "Unwraps the item."),
        ("box", "fallback;", "The fallback item."),
        ("box", "fallback();", "Returns the fallback item."),
        ("proxy", "value;", "The wrapped item."),
        ("proxy", "unwrap();", "Unwraps the item."),
        ("proxy", "fallback;", "The fallback item."),
        ("proxy", "fallback();", "Returns the fallback item."),
    ] {
        let offset = content.find(&format!("${receiver}->{member}")).unwrap();
        let position = offset_position(content, offset + receiver.len() + 3);
        let hover = hover_at(
            &backend,
            "file:///test.php",
            content,
            position.line,
            position.character,
        )
        .expect("hover on the documented member");
        let text = hover_text(&hover);
        assert!(text.contains(description), "{text}");
        let type_line = if !member.contains('(') {
            "**var** `Item`"
        } else {
            "**return** `Item`"
        };
        assert!(text.contains(type_line), "{text}");
    }
}

#[test]
fn hover_phpdoc_descriptions_keep_real_members_documentation() {
    let backend = create_test_backend();
    let content = r#"<?php
/**
 * @property string $label The magic label.
 * @method string render() The magic render.
 */
class Post {
    /** The real label. */
    public string $label = '';
    /** The real render. */
    public function render(): string { return ''; }
}
$post = new Post();
echo $post->label;
echo $post->render();
"#;
    for (needle, description) in [
        ("label;", "The real label."),
        ("render();", "The real render."),
    ] {
        let text = hover_last(&backend, content, needle);
        assert!(text.contains(description), "{text}");
        assert!(!text.contains("The magic"), "{text}");
    }
}

#[test]
fn hover_phpdoc_descriptions_update_after_an_edit() {
    let backend = create_test_backend();
    let source = r#"<?php
/**
 * @property string $label DESCRIPTION
 * @method string render() DESCRIPTION
 */
class Post {}
$post = new Post();
echo $post->label;
echo $post->render();
"#;
    for description in ["Original description.", "Updated description.", ""] {
        let content = source.replace("DESCRIPTION", description);
        for needle in ["label;", "render();"] {
            let text = hover_last(&backend, &content, needle);
            assert!(text.contains(description), "{text}");
            for stale in ["Original description.", "Updated description."] {
                if stale != description {
                    assert!(!text.contains(stale), "{text}");
                }
            }
        }
    }
}

#[test]
fn hover_phpdoc_description_survives_type_alias_substitution() {
    let backend = create_test_backend();
    let content = r#"<?php
/**
 * @phpstan-type Labels list<string>
 * @property Labels $labels The available labels.
 */
class Post {}
$post = new Post();
echo $post->labels;
"#;
    let text = hover_last(&backend, content, "labels;");
    assert!(text.contains("The available labels."), "{text}");
    assert!(text.contains("**var** `list<string>`"), "{text}");
}
