//! GLSL parsing is local to the native editor; JSON uses the component's grammar.
use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};
use std::sync::Once;

pub(super) const NAME: &str = "meshcraft-glsl";

fn query() -> String {
    // The upstream query includes a Neovim-only Lua predicate. Tree-sitter's
    // Rust highlighter supports match?; qualifiers use our shared keyword style.
    let query = tree_sitter_glsl::HIGHLIGHTS_QUERY
        .replace("#lua-match?", "#match?")
        .replace("@type.qualifier", "@keyword");
    format!(
        "((identifier) @keyword (#eq? @keyword \"discard\"))\n[(true) (false)] @constant.builtin\n((identifier) @variable.special (#match? @variable.special \"^(u_|gl_)\"))\n((identifier) @variable.special (#eq? @variable.special \"time\"))\n{query}"
    )
}

pub(super) fn register() {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        LanguageRegistry::singleton().register(
            NAME,
            &LanguageConfig::new(
                NAME,
                tree_sitter_glsl::LANGUAGE_GLSL.into(),
                vec![],
                &query(),
                "",
                "",
            ),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_component::highlighter::{HighlightTheme, SyntaxHighlighter};
    use ropey::Rope;

    #[test]
    fn glsl_highlights_qualifiers_types_hooks_uniforms_and_literals() {
        register();
        let source = "// u_tint\nvec4 proceduralColor(inout vec3 position) { if (true) discard; return vec4(u_tint * sin(time), 1.0); }";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_glsl::LANGUAGE_GLSL.into())
            .unwrap();
        assert!(!parser.parse(source, None).unwrap().root_node().has_error());
        let mut highlighter = SyntaxHighlighter::new(NAME);
        highlighter.update(None, &Rope::from(source));
        let theme =
            super::super::super::super::scripts::language::theme(&HighlightTheme::default_dark());
        let styles = highlighter.styles(&(0..source.len()), &theme);
        for (token, kind) in [
            ("// u_tint", "comment"),
            ("vec4", "type"),
            ("inout", "keyword"),
            ("return", "keyword"),
            ("discard", "keyword"),
            ("true", "constant.builtin"),
            ("proceduralColor", "function"),
            ("sin", "function"),
            ("time", "variable.special"),
            ("1.0", "number"),
        ] {
            let offset = source.find(token).unwrap();
            assert!(
                styles
                    .iter()
                    .any(|(range, style)| range.contains(&offset)
                        && Some(*style) == theme.style(kind)),
                "{token}: {styles:?}"
            );
        }
        let offset = source.rfind("u_tint").unwrap();
        assert!(styles.iter().any(|(range, style)| range.contains(&offset)
            && Some(*style) == theme.style("variable.special")));
    }
}
