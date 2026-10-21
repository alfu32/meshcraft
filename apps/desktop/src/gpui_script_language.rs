//! Local JavaScript parsing and API help. Shares the completion/MCP catalog.
use crate::script_api::{Entry, catalog};
use gpui::{App, Task, Window};
use gpui_component::{
    highlighter::{HighlightTheme, LanguageConfig, LanguageRegistry},
    input::{HoverProvider, RopeExt},
};
use ropey::Rope;
use std::{
    cell::RefCell,
    ops::Range,
    sync::{LazyLock, Once},
};
use tree_sitter::{Node, Parser, Tree};

pub(super) const NAME: &str = "meshcraft-javascript";

fn highlight_query() -> String {
    let mut query = include_str!("script_highlights.scm").to_owned();
    // Match syntax nodes rather than arbitrary text: comments/strings stay untouched.
    for entry in catalog() {
        let name = entry.name;
        let style = if !name.contains('.') && name.chars().next().is_some_and(char::is_uppercase) {
            "type"
        } else if entry.signature.contains('(') {
            "function"
        } else {
            "variable.special"
        };
        if let Some((object, property)) = name.split_once('.') {
            for object in if object == "this" {
                vec!["this", "instance"]
            } else {
                vec![object]
            } {
                let kind = if object == "this" {
                    "this"
                } else {
                    "identifier"
                };
                query.push_str(&format!(
                    "\n((member_expression object: ({kind}) @_object property: (property_identifier) @{style}) (#eq? @_object {object:?}) (#eq? @{style} {property:?}))\n"
                ));
            }
        } else if name != "this" {
            query.push_str(&format!(
                "\n((identifier) @{style} (#eq? @{style} {name:?}))\n"
            ));
        }
    }
    query.push_str("\n(this) @variable.special\n");
    query
}

pub(super) fn register() {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        LanguageRegistry::singleton().register(
            NAME,
            &LanguageConfig::new(
                NAME,
                tree_sitter_javascript::LANGUAGE.into(),
                vec![],
                &highlight_query(),
                "",
                "",
            ),
        );
    });
}

/// The component's minimal default theme only colors JSON categories.
/// Supply the JavaScript/API categories as well, preserving editor chrome.
pub(super) fn theme(base: &HighlightTheme) -> HighlightTheme {
    let mut theme = base.clone();
    for (slot, color) in [
        (&mut theme.style.syntax.keyword, "#c792ea"),
        (&mut theme.style.syntax.type_, "#82d8ce"),
        (&mut theme.style.syntax.function, "#82baff"),
        (&mut theme.style.syntax.variable_special, "#f6c177"),
        (&mut theme.style.syntax.number, "#e5a678"),
        (&mut theme.style.syntax.string, "#b7d78c"),
        (&mut theme.style.syntax.comment, "#8c99aa"),
        (&mut theme.style.syntax.operator, "#b8c9dc"),
    ] {
        *slot = Some(
            serde_json::from_value(serde_json::json!({"color": color}))
                .expect("valid built-in syntax color"),
        );
    }
    theme
}

#[derive(Default)]
struct Parsed {
    source: String,
    tree: Option<Tree>,
}
#[derive(Default)]
pub(super) struct ApiHover {
    parsed: RefCell<Parsed>,
}

fn access(node: Node<'_>, text: &str) -> Option<String> {
    match node.kind() {
        "identifier" | "this" | "property_identifier" => Some(text[node.byte_range()].to_owned()),
        "member_expression" => Some(format!(
            "{}.{}",
            access(node.child_by_field_name("object")?, text)?,
            access(node.child_by_field_name("property")?, text)?
        )),
        _ => None,
    }
}

fn symbol(text: &str, tree: &Tree, offset: usize) -> Option<(Range<usize>, Entry)> {
    if offset >= text.len() || !text.is_char_boundary(offset) {
        return None;
    }
    let end = offset + text[offset..].chars().next()?.len_utf8();
    let node = tree
        .root_node()
        .named_descendant_for_byte_range(offset, end)?;
    if !matches!(node.kind(), "identifier" | "this" | "property_identifier") {
        return None;
    }
    // Resolve a member only when hovering its property. Hovering `model` itself
    // still describes `model`, even in `model.getObject(...)`.
    let target = if node.kind() == "property_identifier" {
        node.parent().filter(|p| p.kind() == "member_expression")?
    } else {
        node
    };
    let name = access(target, text)?;
    let canonical = name
        .strip_prefix("instance.")
        .map(|p| format!("this.{p}"))
        .unwrap_or_else(|| name.clone());
    static ENTRIES: LazyLock<Vec<Entry>> = LazyLock::new(catalog);
    let mut entry = ENTRIES.iter().find(|e| e.name == canonical)?.clone();
    if name != canonical {
        entry.name = name;
        entry.signature = entry.signature.replacen("this.", "instance.", 1);
    }
    Some((node.byte_range(), entry))
}

impl ApiHover {
    fn lookup(&self, text: &str, offset: usize) -> Option<(Range<usize>, Entry)> {
        let mut parsed = self.parsed.borrow_mut();
        if parsed.tree.is_none() || parsed.source != text {
            let mut parser = Parser::new();
            parser
                .set_language(&tree_sitter_javascript::LANGUAGE.into())
                .ok()?;
            parsed.tree = parser.parse(text, None);
            parsed.source = text.to_owned();
        }
        symbol(text, parsed.tree.as_ref()?, offset)
    }
}
impl HoverProvider for ApiHover {
    fn hover(
        &self,
        text: &Rope,
        offset: usize,
        _: &mut Window,
        _: &mut App,
    ) -> Task<anyhow::Result<Option<lsp_types::Hover>>> {
        let hover = self
            .lookup(&text.to_string(), offset)
            .map(|(range, entry)| lsp_types::Hover {
                contents: lsp_types::HoverContents::Markup(lsp_types::MarkupContent {
                    kind: lsp_types::MarkupKind::Markdown,
                    value: format!(
                        "```javascript\n{}\n```\n\n{}\n\nExample:\n```javascript\n{}\n```",
                        entry.signature, entry.help, entry.insert
                    ),
                }),
                range: Some(lsp_types::Range::new(
                    text.offset_to_position(range.start),
                    text.offset_to_position(range.end),
                )),
            });
        Task::ready(Ok(hover))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hover_resolves_members_aliases_and_unicode_offsets() {
        let api = ApiHover::default();
        let text = "// π\nconst p = new Point(); model . getObject(this.id); instance.geometry;";
        for (token, expected) in [
            ("Point", "Point"),
            ("getObject", "model.getObject"),
            ("id)", "this.id"),
            ("geometry", "instance.geometry"),
        ] {
            let (range, entry) = api.lookup(text, text.find(token).unwrap()).unwrap();
            assert_eq!(entry.name, expected);
            assert!(!entry.help.is_empty());
            assert!(text.is_char_boundary(range.start));
        }
        assert!(api.lookup(text, text.len()).is_none());
    }
    #[test]
    fn hover_excludes_literals_comments_unrelated_members_and_stale_text() {
        let api = ApiHover::default();
        for text in [
            "// Point",
            "/* Point */",
            "'Point'",
            "`Point`",
            "/Point/",
            "other.Point",
            "other.line()",
            "model.unknown",
        ] {
            let offset = text
                .find("Point")
                .or_else(|| text.find("line"))
                .unwrap_or(text.find("unknown").unwrap_or(0));
            assert!(api.lookup(text, offset).is_none(), "{text}");
        }
        assert!(api.lookup("Point", 0).is_some());
        assert!(api.lookup("'Point'", 1).is_none());
        assert!(api.lookup("`x ${cube()}`", 5).is_some());
    }
    #[test]
    fn javascript_and_api_query_produce_distinct_highlights() {
        use gpui_component::highlighter::{HighlightTheme, SyntaxHighlighter};
        register();
        let body = "const p = new Point(); return model . getObject(this.id); // cube\n'Point'";
        // Exercise both a single rope chunk and node ranges after several chunks.
        for prefix in [String::new(), format!("/* {} */\n", "x".repeat(5000))] {
            let source = format!("{prefix}{body}");
            let rope = Rope::from(source.as_str());
            let mut highlighter = SyntaxHighlighter::new(NAME);
            highlighter.update(None, &rope);
            let theme = super::theme(&HighlightTheme::default_dark());
            let styles = highlighter.styles(&(0..source.len()), &theme);
            for (token, kind) in [
                ("const", "keyword"),
                ("Point", "type"),
                ("getObject", "function"),
                ("id)", "variable.special"),
            ] {
                let offset = source.find(token).unwrap();
                assert!(
                    styles
                        .iter()
                        .any(|(r, style)| r.contains(&offset) && Some(*style) == theme.style(kind)),
                    "missing {token}: {styles:?}"
                );
            }
        }
    }
}
