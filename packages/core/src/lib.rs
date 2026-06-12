#![deny(clippy::all)]

mod compiler;
mod directive;
mod error;
mod expr;

#[cfg(test)]
mod bench;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use fast_glob::glob_match;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use oxc_allocator::Allocator;
use oxc_ast_visit::Visit;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use rayon::prelude::*;
use rustc_hash::FxHashSet;
use walkdir::WalkDir;

use compiler::Compiler;
use directive::may_contain_fuma_translate;
pub(crate) use error::{join_errors, AnalysisError, FileAnalysis};

#[napi(object)]
pub struct CompileOutput {
    pub translation_keys: Vec<String>,
}

pub(crate) fn analyze_source(file: &str, source_type: SourceType, source: &str) -> FileAnalysis {
    if !may_contain_fuma_translate(source) {
        return FileAnalysis::empty();
    }

    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, source_type).parse();

    if parsed.panicked {
        return FileAnalysis::error(format!("{file}: parser panicked"));
    }

    if !parsed.errors.is_empty() {
        return FileAnalysis::error(
            parsed
                .errors
                .iter()
                .map(|error| format!("{file}: {}", error.message))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    let semantic_result = SemanticBuilder::new().build(&parsed.program);

    if !semantic_result.errors.is_empty() {
        let mut analysis = FileAnalysis::empty();
        analysis
            .errors
            .extend(semantic_result.errors.iter().map(|error| AnalysisError {
                message: format!("{file}: {error}"),
            }));
        return analysis;
    }

    let mut compiler = Compiler::new(
        &semantic_result.semantic,
        &parsed.program.comments,
        source,
        file,
    );
    compiler.visit_program(&parsed.program);
    let (keys, errors) = compiler.into_parts();

    FileAnalysis { keys, errors }
}

fn glob_walk_root(pattern: &str) -> PathBuf {
    let glob_start = pattern
        .char_indices()
        .find(|(_, ch)| matches!(ch, '*' | '?' | '[' | '{'))
        .map_or(pattern.len(), |(idx, _)| idx);
    let base = &pattern[..glob_start];

    if let Some(pos) = base.rfind('/') {
        PathBuf::from(&base[..pos])
    } else if let Some(pos) = base.rfind('\\') {
        PathBuf::from(&base[..pos])
    } else {
        PathBuf::from(".")
    }
}

pub(crate) fn collect_files(input: &[String]) -> std::result::Result<Vec<PathBuf>, AnalysisError> {
    let patterns: Vec<&[u8]> = input.iter().map(String::as_bytes).collect();
    let roots: FxHashSet<PathBuf> = input
        .iter()
        .map(|pattern| glob_walk_root(pattern))
        .collect();
    let mut files = FxHashSet::default();

    for root in roots {
        for entry in WalkDir::new(root).follow_links(false) {
            let entry = entry.map_err(|error| AnalysisError {
                message: error.to_string(),
            })?;

            if entry.file_type().is_file() {
                let path_bytes = entry.path().as_os_str().as_encoded_bytes();
                if patterns
                    .iter()
                    .any(|pattern| glob_match(pattern, path_bytes))
                {
                    files.insert(entry.into_path());
                }
            }
        }
    }

    Ok(files.into_iter().collect())
}

pub(crate) fn analyze_file(path: &Path) -> FileAnalysis {
    let file = path.to_string_lossy();

    let source_type = match SourceType::from_path(path) {
        Ok(source_type) => source_type,
        Err(error) => return FileAnalysis::error(format!("{file}: {error}")),
    };

    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => return FileAnalysis::error(format!("{file}: {error}")),
    };

    analyze_source(&file, source_type, &source)
}

pub(crate) fn compile_files(input: &[String]) -> std::result::Result<CompileOutput, AnalysisError> {
    let files = collect_files(input)?;

    let analyses: Vec<FileAnalysis> = files
        .into_par_iter()
        .map(|path| analyze_file(path.as_path()))
        .collect();

    let mut keys = FxHashSet::default();
    let mut errors = Vec::new();

    for analysis in analyses {
        keys.extend(analysis.keys);
        errors.extend(analysis.errors);
    }

    if !errors.is_empty() {
        return Err(join_errors(errors));
    }

    let mut translation_keys = Vec::with_capacity(keys.len());
    translation_keys.extend(keys);
    translation_keys.sort_unstable();

    Ok(CompileOutput { translation_keys })
}

#[napi]
pub fn compile_sync(input: Vec<String>) -> Result<CompileOutput> {
    compile_files(&input).map_err(|error| Error::from_reason(error.message))
}
