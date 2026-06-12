use oxc_ast::ast::{Comment, CommentKind};

pub(crate) const FUMA_TRANSLATE_REACT: &str = "@fuma-translate/react";
pub(crate) const FUMA_TRANSLATE_DIRECTIVE: &str = "@fuma-translate";

pub(crate) fn may_contain_fuma_translate(source: &str) -> bool {
    source.contains(FUMA_TRANSLATE_REACT) || source.contains(FUMA_TRANSLATE_DIRECTIVE)
}

fn comment_body<'a>(source: &'a str, comment: &Comment) -> &'a str {
    let raw = &source[comment.span.start as usize..comment.span.end as usize];

    match comment.kind {
        CommentKind::Line => raw.strip_prefix("//").unwrap_or(raw).trim(),
        CommentKind::SingleLineBlock | CommentKind::MultiLineBlock => raw
            .trim_start_matches("/*")
            .trim_end_matches("*/")
            .trim(),
    }
}

fn is_directive_comment_body(body: &str) -> bool {
    let body = body.trim();
    let Some(suffix) = body.strip_prefix(FUMA_TRANSLATE_DIRECTIVE) else {
        return false;
    };

    suffix.is_empty() || !suffix.starts_with('/')
}

fn line_has_directive_comment(line: &str) -> bool {
    if let Some(block_start) = line.find("/*") {
        let block = line[block_start..]
            .trim_start_matches("/*")
            .trim_end_matches("*/")
            .trim();
        if is_directive_comment_body(block) {
            return true;
        }
    }

    line.split("//").skip(1).any(is_directive_comment_body)
}

pub(crate) fn has_adjacent_jsx_directive(source: &str, element_start: u32) -> bool {
    let idx = element_start as usize;
    let mut line_start = source[..idx].rfind('\n').map(|i| i + 1).unwrap_or(0);

    if source[line_start..idx].trim().is_empty() && line_start > 0 {
        line_start = source[..line_start.saturating_sub(1)]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);
    }

    line_has_directive_comment(&source[line_start..idx])
}

pub(crate) fn has_fuma_translate_directive(comments: &[Comment], source: &str, node_start: u32) -> bool {
    comments.iter().any(|comment| {
        if !comment.is_leading() {
            return false;
        }

        if !is_directive_comment_body(comment_body(source, comment)) {
            return false;
        }

        if comment.span.end > node_start {
            return false;
        }

        source.as_bytes()[comment.span.end as usize..node_start as usize]
            .iter()
            .all(|byte| byte.is_ascii_whitespace())
    })
}
