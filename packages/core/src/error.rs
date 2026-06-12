use oxc_span::Span;

pub(crate) struct AnalysisError {
    pub(crate) message: String,
}

pub(crate) struct FileAnalysis {
    pub(crate) keys: rustc_hash::FxHashSet<String>,
    pub(crate) errors: Vec<AnalysisError>,
}

impl FileAnalysis {
    pub(crate) fn empty() -> Self {
        Self {
            keys: rustc_hash::FxHashSet::default(),
            errors: Vec::new(),
        }
    }

    pub(crate) fn error(message: impl Into<String>) -> Self {
        Self {
            keys: rustc_hash::FxHashSet::default(),
            errors: vec![AnalysisError {
                message: message.into(),
            }],
        }
    }
}

pub(crate) fn join_errors(errors: Vec<AnalysisError>) -> AnalysisError {
    AnalysisError {
        message: errors
            .into_iter()
            .map(|error| error.message)
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

pub(crate) fn fail(
    source: &str,
    file: &str,
    span: Span,
    message: impl Into<String>,
) -> AnalysisError {
    AnalysisError {
        message: format!(
            "{}:{}: {}",
            file,
            format_location(source, span.start),
            message.into()
        ),
    }
}

fn format_location(source: &str, offset: u32) -> String {
    let offset = offset as usize;
    let mut line = 1usize;
    let mut column = 1usize;

    for &byte in &source.as_bytes()[..offset.min(source.len())] {
        if byte == b'\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    format!("{line}:{column}")
}
