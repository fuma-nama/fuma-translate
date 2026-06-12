use oxc_ast::ast::Comment;
use oxc_span::Span;

pub(crate) const FUMA_TRANSLATE_REACT: &str = "@fuma-translate/react";
pub(crate) const FUMA_TRANSLATE_DIRECTIVE: &str = "@fuma-translate";

pub(crate) fn may_contain_fuma_translate(source: &str) -> bool {
    source.contains(FUMA_TRANSLATE_REACT) || source.contains(FUMA_TRANSLATE_DIRECTIVE)
}

fn is_directive_comment_body(body: &str) -> bool {
    let body = body.trim();
    let Some(suffix) = body.strip_prefix(FUMA_TRANSLATE_DIRECTIVE) else {
        return false;
    };

    suffix.is_empty() || !suffix.starts_with('/')
}

fn preceding_comment(comments: &[Comment], node_start: u32) -> Option<&Comment> {
    // OXC stores comments in source order, so only the nearest prior comment can
    // be adjacent after the whitespace-only gap check.
    comments
        .iter()
        .rev()
        .find(|comment| comment.span.end <= node_start)
}

pub(crate) fn has_fuma_translate_directive(
    comments: &[Comment],
    source: &str,
    node_start: u32,
) -> bool {
    let Some(comment) = preceding_comment(comments, node_start) else {
        return false;
    };

    comment.is_leading()
        && is_directive_comment_body(comment.content_span().source_text(source))
        && Span::new(comment.span.end, node_start)
            .source_text(source)
            .bytes()
            .all(|byte| byte.is_ascii_whitespace())
}

pub(crate) fn has_adjacent_jsx_directive(
    comments: &[Comment],
    source: &str,
    element_start: u32,
) -> bool {
    let Some(comment) = preceding_comment(comments, element_start) else {
        return false;
    };

    is_directive_comment_body(comment.content_span().source_text(source))
        && Span::new(comment.span.end, element_start)
            .source_text(source)
            .trim()
            == "}"
}
