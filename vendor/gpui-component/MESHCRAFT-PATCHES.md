# Meshcraft component patch

Vendored from the published `gpui-component` 0.5.1 crate. Original source and
Apache-2.0 license are preserved. Optional language features remain disabled;
Meshcraft registers only its JavaScript grammar separately.

Local change: `src/highlighter/highlighter.rs` uses `RopeSlice::chunks()` for
Tree-sitter node text. The upstream cursor advanced before yielding, omitting
its first chunk, and did not clip yielded chunks to node boundaries. As a
result, name predicates failed for short scripts and could read unrelated text
in longer scripts. Meshcraft's editor tests cover actual highlighted styles,
including single-chunk scripts and symbols after multiple rope chunks.
