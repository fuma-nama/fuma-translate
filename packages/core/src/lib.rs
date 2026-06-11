#![deny(clippy::all)]

use std::path::{Path, PathBuf};

use napi::bindgen_prelude::*;
use napi_derive::napi;
use oxc_allocator::Allocator;
use oxc_ast::ast::{BindingPattern, CallExpression, Expression, ObjectPropertyKind, PropertyKey, VariableDeclarator};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_semantic::{Semantic, SemanticBuilder};
use oxc_span::{GetSpan, SourceType, Span};
use oxc_syntax::symbol::SymbolId;
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};

type HookNoteBranches = Vec<Option<String>>;

#[napi(object)]
pub struct CompileOutput {
    pub translation_keys: Vec<String>,
}

struct AnalysisError {
    message: String,
}

struct FileAnalysis {
    keys: FxHashSet<String>,
    errors: Vec<AnalysisError>,
}

fn format_location(source: &str, offset: u32) -> String {
    let mut line = 1;
    let mut column = 1;

    for ch in source.chars().take(offset as usize) {
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    format!("{line}:{column}")
}

fn fail(source: &str, file: &str, span: Span, message: impl Into<String>) -> AnalysisError {
    AnalysisError {
        message: format!(
            "{}:{}: {}",
            file,
            format_location(source, span.start),
            message.into()
        ),
    }
}

fn unwrap_expression<'a>(mut expr: &'a Expression<'a>) -> &'a Expression<'a> {
    loop {
        match expr {
            Expression::ParenthesizedExpression(wrapped) => expr = &wrapped.expression,
            Expression::TSAsExpression(wrapped) => expr = &wrapped.expression,
            Expression::TSSatisfiesExpression(wrapped) => expr = &wrapped.expression,
            Expression::TSTypeAssertion(wrapped) => expr = &wrapped.expression,
            _ => return expr,
        }
    }
}

fn collect_static_strings<'a>(
    expr: &Expression<'a>,
    source: &str,
    file: &str,
) -> std::result::Result<Vec<String>, AnalysisError> {
    let expr = unwrap_expression(expr);

    match expr {
        Expression::StringLiteral(literal) => Ok(vec![literal.value.to_string()]),
        Expression::TemplateLiteral(template) => {
            if !template.expressions.is_empty() {
                return Err(fail(
                    source,
                    file,
                    template.span,
                    "translation key must be a static string",
                ));
            }

            Ok(vec![template
                .quasis
                .iter()
                .map(|quasi| {
                    quasi
                        .value
                        .cooked
                        .as_ref()
                        .map(|cooked| cooked.to_string())
                        .unwrap_or_else(|| quasi.value.raw.to_string())
                })
                .collect::<String>()])
        }
        Expression::ConditionalExpression(conditional) => {
            let mut values = Vec::new();
            let mut errors = Vec::new();

            match collect_static_strings(&conditional.consequent, source, file) {
                Ok(branch_values) => values.extend(branch_values),
                Err(error) => errors.push(error),
            }

            match collect_static_strings(&conditional.alternate, source, file) {
                Ok(branch_values) => values.extend(branch_values),
                Err(error) => errors.push(error),
            }

            if !errors.is_empty() {
                return Err(join_errors(errors));
            }

            Ok(values)
        }
        _ => Err(fail(
            source,
            file,
            expr.span(),
            "translation key must be a static string",
        )),
    }
}

fn get_note_property<'a>(
    properties: &'a [ObjectPropertyKind<'a>],
) -> Option<&'a oxc_ast::ast::ObjectProperty<'a>> {
    properties.iter().find_map(|prop| {
        let ObjectPropertyKind::ObjectProperty(property) = prop else {
            return None;
        };

        if property.kind != oxc_ast::ast::PropertyKind::Init {
            return None;
        }

        match &property.key {
            PropertyKey::StaticIdentifier(ident) if ident.name == "note" => Some(&**property),
            PropertyKey::StringLiteral(literal) if literal.value == "note" => Some(&**property),
            _ => None,
        }
    })
}

fn collect_notes<'a>(
    expr: Option<&Expression<'a>>,
    source: &str,
    file: &str,
) -> std::result::Result<HookNoteBranches, AnalysisError> {
    let Some(expr) = expr else {
        return Ok(vec![None]);
    };

    let expr = unwrap_expression(expr);

    if let Expression::ConditionalExpression(conditional) = expr {
        let mut notes = Vec::new();
        let mut errors = Vec::new();

        match collect_notes(Some(&conditional.consequent), source, file) {
            Ok(branch_notes) => notes.extend(branch_notes),
            Err(error) => errors.push(error),
        }

        match collect_notes(Some(&conditional.alternate), source, file) {
            Ok(branch_notes) => notes.extend(branch_notes),
            Err(error) => errors.push(error),
        }

        if !errors.is_empty() {
            return Err(join_errors(errors));
        }

        return Ok(notes);
    }

    let Expression::ObjectExpression(object) = expr else {
        return Err(fail(
            source,
            file,
            expr.span(),
            "translation options must be a static object",
        ));
    };

    for prop in &object.properties {
        if matches!(prop, ObjectPropertyKind::SpreadProperty(_)) {
            return Err(fail(
                source,
                file,
                prop.span(),
                "translation options cannot use spread properties",
            ));
        }
    }

    let Some(note_prop) = get_note_property(&object.properties) else {
        return Ok(vec![None]);
    };

    if note_prop.shorthand {
        return Err(fail(
            source,
            file,
            note_prop.span,
            "translation note must be a static string",
        ));
    }

    collect_static_strings(&note_prop.value, source, file).map(|notes| notes.into_iter().map(Some).collect())
}

fn parse_use_translations_call<'a>(
    call: &CallExpression<'a>,
    source: &str,
    file: &str,
) -> std::result::Result<HookNoteBranches, AnalysisError> {
    if call.arguments.is_empty() {
        return Ok(vec![None]);
    }

    if call.arguments.len() > 1 {
        return Err(fail(
            source,
            file,
            call.span,
            "useTranslations accepts at most one options argument",
        ));
    }

    if call.arguments[0].is_spread() {
        return Err(fail(
            source,
            file,
            call.arguments[0].span(),
            "useTranslations options must be a static object",
        ));
    }

    collect_notes(call.arguments[0].as_expression(), source, file)
}

fn parse_from_translations_call<'a>(
    call: &CallExpression<'a>,
    source: &str,
    file: &str,
) -> std::result::Result<HookNoteBranches, AnalysisError> {
    if call.arguments.is_empty() {
        return Err(fail(
            source,
            file,
            call.span,
            "fromTranslations requires a translations object",
        ));
    }

    if call.arguments.len() > 2 {
        return Err(fail(
            source,
            file,
            call.span,
            "fromTranslations accepts at most two arguments",
        ));
    }

    if call.arguments.len() == 1 {
        return Ok(vec![None]);
    }

    if call.arguments[1].is_spread() {
        return Err(fail(
            source,
            file,
            call.arguments[1].span(),
            "fromTranslations options must be a static object",
        ));
    }

    collect_notes(call.arguments[1].as_expression(), source, file)
}

fn parse_translations_hook_call<'a>(
    expr: &Expression<'a>,
    source: &str,
    file: &str,
) -> std::result::Result<Option<HookNoteBranches>, AnalysisError> {
    let Expression::CallExpression(call) = unwrap_expression(expr) else {
        return Ok(None);
    };

    let Expression::Identifier(callee) = unwrap_expression(&call.callee) else {
        return Ok(None);
    };

    match callee.name.as_str() {
        "useTranslations" => parse_use_translations_call(call, source, file).map(Some),
        "fromTranslations" => parse_from_translations_call(call, source, file).map(Some),
        _ => Ok(None),
    }
}

fn join_errors(errors: Vec<AnalysisError>) -> AnalysisError {
    AnalysisError {
        message: errors
            .into_iter()
            .map(|error| error.message)
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn encode_key(text: &str, hook_note: Option<&str>, call_note: Option<&str>) -> String {
    let mut capacity = text.len();
    if let Some(note) = hook_note {
        capacity += note.len() + 2;
    }
    if let Some(note) = call_note {
        capacity += note.len() + 2;
    }

    let mut key = String::with_capacity(capacity);
    key.push_str(text);
    if let Some(note) = hook_note {
        key.push('(');
        key.push_str(note);
        key.push(')');
    }
    if let Some(note) = call_note {
        key.push('(');
        key.push_str(note);
        key.push(')');
    }
    key
}

fn push_encoded_keys(
    keys: &mut FxHashSet<String>,
    hook_symbols: &FxHashMap<SymbolId, HookNoteBranches>,
    hook_symbol_id: Option<SymbolId>,
    text: &str,
    call_notes: &[Option<String>],
) {
    let hook_notes: &[Option<String>] = hook_symbol_id
        .and_then(|symbol_id| hook_symbols.get(&symbol_id))
        .map(HookNoteBranches::as_slice)
        .unwrap_or(&[None][..]);

    for hook_note in hook_notes {
        for call_note in call_notes {
            keys.insert(encode_key(
                text,
                hook_note.as_deref(),
                call_note.as_deref(),
            ));
        }
    }
}

struct Compiler<'a> {
    semantic: &'a Semantic<'a>,
    source: &'a str,
    file: &'a str,
    strict: bool,
    hook_symbols: FxHashMap<SymbolId, HookNoteBranches>,
    keys: FxHashSet<String>,
    errors: Vec<AnalysisError>,
}

impl<'a> Compiler<'a> {
    fn new(semantic: &'a Semantic<'a>, source: &'a str, file: &'a str, strict: bool) -> Self {
        Self {
            semantic,
            source,
            file,
            strict,
            hook_symbols: FxHashMap::default(),
            keys: FxHashSet::default(),
            errors: Vec::new(),
        }
    }

    fn push_error(&mut self, error: AnalysisError) {
        self.errors.push(error);
    }

    fn hook_error(&mut self, from_hook: bool, span: Span, message: impl Into<String>) {
        if from_hook {
            self.push_error(fail(self.source, self.file, span, message));
        }
    }

    fn analyze_call(&mut self, call: &CallExpression<'a>) {
        let Expression::Identifier(callee) = unwrap_expression(&call.callee) else {
            return;
        };

        let reference = self.semantic.scoping().get_reference(callee.reference_id());
        let hook_symbol_id = reference
            .symbol_id()
            .filter(|symbol_id| self.hook_symbols.contains_key(symbol_id));
        let from_hook = hook_symbol_id.is_some();

        if self.strict && !from_hook {
            return;
        }

        if call.arguments.is_empty() {
            self.hook_error(
                from_hook,
                call.span,
                "translation call requires a static string argument",
            );
            return;
        }

        if call.arguments.len() > 2 {
            self.hook_error(
                from_hook,
                call.span,
                "translation call accepts at most two arguments",
            );
            return;
        }

        if call.arguments[0].is_spread() {
            self.hook_error(
                from_hook,
                call.arguments[0].span(),
                "translation key must be a static string",
            );
            return;
        }

        let Some(first_arg) = call.arguments[0].as_expression() else {
            self.hook_error(
                from_hook,
                call.span,
                "translation key must be a static string",
            );
            return;
        };

        let texts = match collect_static_strings(first_arg, self.source, self.file) {
            Ok(texts) => texts,
            Err(_error) if !from_hook => return,
            Err(error) => {
                self.push_error(error);
                return;
            }
        };

        let call_notes = if call.arguments.len() > 1 {
            if call.arguments[1].is_spread() {
                self.hook_error(
                    from_hook,
                    call.arguments[1].span(),
                    "translation options must be a static object",
                );
                return;
            }

            match collect_notes(call.arguments[1].as_expression(), self.source, self.file) {
                Ok(notes) => notes,
                Err(_error) if !from_hook => return,
                Err(error) => {
                    self.push_error(error);
                    return;
                }
            }
        } else {
            vec![None]
        };

        for text in texts {
            push_encoded_keys(
                &mut self.keys,
                &self.hook_symbols,
                hook_symbol_id,
                &text,
                &call_notes,
            );
        }
    }
}

impl<'a> Visit<'a> for Compiler<'a> {
    fn visit_variable_declarator(&mut self, decl: &VariableDeclarator<'a>) {
        if let Some(init) = &decl.init {
            match parse_translations_hook_call(init, self.source, self.file) {
                Ok(Some(notes)) => {
                    if let BindingPattern::BindingIdentifier(ident) = &decl.id {
                        self.hook_symbols.insert(ident.symbol_id(), notes);
                    }
                }
                Ok(None) => {}
                Err(error) => self.push_error(error),
            }
        }

        walk::walk_variable_declarator(self, decl);
    }

    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        if let Expression::Identifier(callee) = unwrap_expression(&call.callee) {
            if callee.name == "t" {
                self.analyze_call(call);
            }
        }

        walk::walk_call_expression(self, call);
    }
}

fn analyze_source(
    file: &str,
    source_type: SourceType,
    source: &str,
    strict: bool,
) -> FileAnalysis {
    let mut errors = Vec::new();
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, source_type).parse();

    if parsed.panicked {
        errors.push(AnalysisError {
            message: format!("{file}: parser panicked"),
        });
        return FileAnalysis {
            keys: FxHashSet::default(),
            errors,
        };
    }

    if !parsed.errors.is_empty() {
        errors.push(AnalysisError {
            message: parsed
                .errors
                .iter()
                .map(|error| format!("{file}: {}", error.message))
                .collect::<Vec<_>>()
                .join("\n"),
        });
        return FileAnalysis {
            keys: FxHashSet::default(),
            errors,
        };
    }

    let semantic_result = SemanticBuilder::new().build(&parsed.program);

    if !semantic_result.errors.is_empty() {
        errors.extend(semantic_result.errors.iter().map(|error| AnalysisError {
            message: format!("{file}: {error}"),
        }));
        return FileAnalysis {
            keys: FxHashSet::default(),
            errors,
        };
    }

    let mut compiler = Compiler::new(&semantic_result.semantic, source, file, strict);
    compiler.visit_program(&parsed.program);
    errors.extend(compiler.errors);

    FileAnalysis {
        keys: compiler.keys,
        errors,
    }
}

fn collect_files(input: &[String]) -> std::result::Result<Vec<PathBuf>, AnalysisError> {
    let mut files = FxHashSet::default();

    for pattern in input {
        let entries = glob::glob(pattern).map_err(|error| AnalysisError {
            message: error.to_string(),
        })?;

        for entry in entries {
            let path = entry.map_err(|error| AnalysisError {
                message: error.to_string(),
            })?;

            if path.is_file() {
                files.insert(path);
            }
        }
    }

    Ok(files.into_iter().collect())
}

fn analyze_file(path: &Path, strict: bool) -> FileAnalysis {
    let file = path.to_string_lossy();

    let source_type = match SourceType::from_path(path) {
        Ok(source_type) => source_type,
        Err(error) => {
            return FileAnalysis {
                keys: FxHashSet::default(),
                errors: vec![AnalysisError {
                    message: format!("{file}: {error}"),
                }],
            };
        }
    };

    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            return FileAnalysis {
                keys: FxHashSet::default(),
                errors: vec![AnalysisError {
                    message: format!("{file}: {error}"),
                }],
            };
        }
    };

    analyze_source(&file, source_type, &source, strict)
}

#[napi]
pub fn compile_sync(input: Vec<String>, strict: Option<bool>) -> Result<CompileOutput> {
    let strict = strict.unwrap_or(true);
    let files = collect_files(&input).map_err(|error| Error::from_reason(error.message))?;

    let analyses: Vec<FileAnalysis> = files
        .par_iter()
        .map(|path| analyze_file(path, strict))
        .collect();

    let mut errors = Vec::new();
    let mut keys = FxHashSet::default();

    for analysis in analyses {
        keys.extend(analysis.keys);
        errors.extend(analysis.errors);
    }

    if !errors.is_empty() {
        return Err(Error::from_reason(
            errors
                .into_iter()
                .map(|error| error.message)
                .collect::<Vec<_>>()
                .join("\n"),
        ));
    }

    Ok(CompileOutput {
        translation_keys: keys.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> String {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test/fixtures")
            .join(name)
            .to_string_lossy()
            .into_owned()
    }

    fn compile_fixture(name: &str, strict: bool) -> Vec<String> {
        let file = fixture(name);
        let source = std::fs::read_to_string(&file).unwrap();
        let source_type = SourceType::from_path(&file).expect("valid fixture extension");
        let analysis = analyze_source(&file, source_type, &source, strict);
        if !analysis.errors.is_empty() {
            panic!("{}", join_errors(analysis.errors).message);
        }
        let mut keys: Vec<String> = analysis.keys.into_iter().collect();
        keys.sort_unstable();
        keys
    }

    #[test]
    fn basic_fixture() {
        assert_eq!(
            compile_fixture("basic.tsx", true),
            vec![
                "Close(dialog button)".to_string(),
                "Hello".to_string(),
                "Hello {user}".to_string(),
                "Static template".to_string(),
            ]
        );
    }

    #[test]
    fn from_translations_fixture() {
        assert_eq!(
            compile_fixture("from-translations.tsx", true),
            vec![
                "Dashboard(admin panel)".to_string(),
                "Server Hello".to_string(),
            ]
        );
    }

    #[test]
    fn strict_ignored_fixture() {
        assert_eq!(
            compile_fixture("ignored.tsx", true),
            vec!["From hook".to_string(), "Tracked".to_string()]
        );
    }

    #[test]
    fn collects_multiple_errors_in_one_file() {
        let source = r#"
import { useTranslations } from "@fuma-translate/react";

export function Broken() {
  const t = useTranslations();
  const key = "Hello";

  return (
    <>
      {t(key)}
      {t("Save", { ...{ note: "dialog" } })}
    </>
  );
}
"#;
        let analysis = analyze_source(
            "broken.tsx",
            SourceType::tsx(),
            source,
            false,
        );

        assert_eq!(analysis.errors.len(), 2);
        assert!(analysis
            .errors
            .iter()
            .any(|error| error.message.contains("translation key must be a static string")));
        assert!(analysis
            .errors
            .iter()
            .any(|error| error.message.contains("translation options cannot use spread properties")));
    }
}

#[cfg(test)]
mod bench {
    use super::*;
    use std::path::Path;
    use std::time::Instant;

    const BASIC_PATTERN: &str = "/tmp/fuma-translate-bench/files/*.tsx";
    const LARGE_PATTERN: &str = "/tmp/fuma-translate-bench-large/files/*.tsx";

    fn bench_pattern(label: &str, pattern: &str) {
        let sample = pattern.trim_end_matches("*.tsx");
        if !Path::new(sample).is_dir() {
            eprintln!(
                "skip {label}: missing {sample}\n\
                 run: packages/core/test/bench/setup.sh"
            );
            return;
        }

        let t0 = Instant::now();
        let files = match collect_files(&[pattern.to_string()]) {
            Ok(files) => files,
            Err(error) => panic!("collect_files: {}", error.message),
        };
        let collect_ms = t0.elapsed().as_millis();

        let t1 = Instant::now();
        let analyses: Vec<FileAnalysis> = files
            .par_iter()
            .map(|path| analyze_file(path, false))
            .collect();
        let analyze_ms = t1.elapsed().as_millis();

        let t2 = Instant::now();
        let mut keys = FxHashSet::default();
        for analysis in analyses {
            keys.extend(analysis.keys);
        }
        let merge_ms = t2.elapsed().as_millis();

        eprintln!("{label}: {} files, {} keys", files.len(), keys.len());
        eprintln!("  collect: {collect_ms}ms");
        eprintln!("  analyze: {analyze_ms}ms");
        eprintln!("  merge: {merge_ms}ms");
        eprintln!("  total: {}ms", collect_ms + analyze_ms + merge_ms);
    }

    /// Generate inputs with `packages/core/test/bench/setup.sh`, then run:
    /// `cargo test --release bench -- --ignored --nocapture`
    #[test]
    #[ignore = "manual benchmark; requires generated files in /tmp"]
    fn compile_phases() {
        bench_pattern("basic", BASIC_PATTERN);
        bench_pattern("large", LARGE_PATTERN);
    }
}
