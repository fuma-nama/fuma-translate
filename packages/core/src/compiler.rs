use oxc_ast::ast::{
    BindingPattern, CallExpression, Comment, Expression, IdentifierReference, ImportDeclaration,
    ImportDeclarationSpecifier, ImportOrExportKind, JSXAttributeItem, JSXElement, JSXElementName,
    JSXOpeningElement, VariableDeclarator,
};
use oxc_span::GetSpan;
use oxc_ast_visit::{Visit, walk};
use oxc_semantic::Semantic;
use oxc_syntax::symbol::SymbolId;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::directive::{has_adjacent_jsx_directive, has_fuma_translate_directive, FUMA_TRANSLATE_REACT};
use crate::error::{fail, AnalysisError};
use crate::expr::{
    collect_jsx_attribute_notes, collect_jsx_attribute_strings, collect_notes,
    collect_static_strings, fuma_import_from_export_name, get_jsx_note_attribute,
    get_jsx_text_attribute, module_export_name, parse_translations_hook_call,
    FumaImport, HookNoteBranches,
};

pub(crate) struct Compiler<'a> {
    semantic: &'a Semantic<'a>,
    comments: &'a [Comment],
    source: &'a str,
    file: &'a str,
    fuma_imports: FxHashMap<SymbolId, FumaImport>,
    hook_symbols: FxHashMap<SymbolId, HookNoteBranches>,
    keys: FxHashSet<String>,
    errors: Vec<AnalysisError>,
}

impl<'a> Compiler<'a> {
    pub(crate) fn new(
        semantic: &'a Semantic<'a>,
        comments: &'a [Comment],
        source: &'a str,
        file: &'a str,
    ) -> Self {
        Self {
            semantic,
            comments,
            source,
            file,
            fuma_imports: FxHashMap::default(),
            hook_symbols: FxHashMap::default(),
            keys: FxHashSet::default(),
            errors: Vec::new(),
        }
    }

    pub(crate) fn into_parts(self) -> (FxHashSet<String>, Vec<AnalysisError>) {
        (self.keys, self.errors)
    }

    fn fuma_import_for_ident(&self, ident: &IdentifierReference<'a>) -> Option<FumaImport> {
        self.semantic
            .scoping()
            .get_reference(ident.reference_id())
            .symbol_id()
            .and_then(|symbol_id| self.fuma_imports.get(&symbol_id).copied())
    }

    fn hook_symbol_for_ident(&self, ident: &IdentifierReference<'a>) -> Option<SymbolId> {
        self.semantic
            .scoping()
            .get_reference(ident.reference_id())
            .symbol_id()
            .filter(|symbol_id| self.hook_symbols.contains_key(symbol_id))
    }

    fn is_fuma_t_component(&self, name: &JSXElementName<'a>) -> bool {
        let JSXElementName::IdentifierReference(ident) = name else {
            return false;
        };

        self.fuma_import_for_ident(ident) == Some(FumaImport::T)
    }

    fn should_analyze_call(&self, call: &CallExpression<'a>) -> Option<Option<SymbolId>> {
        if let Some(hook_symbol_id) = self.hook_symbol_for_callee(call) {
            return Some(Some(hook_symbol_id));
        }

        if has_fuma_translate_directive(self.comments, self.source, call.span.start) {
            return Some(None);
        }

        None
    }

    fn should_analyze_jsx(&self, element: &JSXElement<'a>) -> bool {
        if self.is_fuma_t_component(&element.opening_element.name) {
            return true;
        }

        if has_fuma_translate_directive(self.comments, self.source, element.span.start) {
            return true;
        }

        has_adjacent_jsx_directive(self.source, element.span.start)
    }

    fn hook_symbol_for_callee(&self, call: &CallExpression<'a>) -> Option<SymbolId> {
        let callee = unwrap_callee(&call.callee);

        if let Expression::Identifier(ident) = callee {
            return self.hook_symbol_for_ident(ident);
        }

        let Expression::StaticMemberExpression(member) = callee else {
            return None;
        };

        if member.property.name != "jsx" {
            return None;
        }

        let Expression::Identifier(ident) = unwrap_callee(&member.object) else {
            return None;
        };

        self.hook_symbol_for_ident(ident)
    }

    fn analyze_call(&mut self, call: &CallExpression<'a>, hook_symbol_id: Option<SymbolId>) {
        if call.arguments.is_empty() {
            self.errors.push(fail(
                self.source,
                self.file,
                call.span,
                "translation call requires a static string argument",
            ));
            return;
        }

        if call.arguments.len() > 2 {
            self.errors.push(fail(
                self.source,
                self.file,
                call.span,
                "translation call accepts at most two arguments",
            ));
            return;
        }

        if call.arguments[0].is_spread() {
            self.errors.push(fail(
                self.source,
                self.file,
                call.arguments[0].span(),
                "translation key must be a static string",
            ));
            return;
        }

        let Some(first_arg) = call.arguments[0].as_expression() else {
            self.errors.push(fail(
                self.source,
                self.file,
                call.span,
                "translation key must be a static string",
            ));
            return;
        };

        let texts = match collect_static_strings(first_arg, self.source, self.file) {
            Ok(texts) => texts,
            Err(error) => {
                self.errors.push(error);
                return;
            }
        };

        let call_notes = if call.arguments.len() > 1 {
            if call.arguments[1].is_spread() {
                self.errors.push(fail(
                    self.source,
                    self.file,
                    call.arguments[1].span(),
                    "translation options must be a static object",
                ));
                return;
            }

            match collect_notes(call.arguments[1].as_expression(), self.source, self.file) {
                Ok(notes) => notes,
                Err(error) => {
                    self.errors.push(error);
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

    fn analyze_jsx(&mut self, opening: &JSXOpeningElement<'a>) {
        for item in &opening.attributes {
            if matches!(item, JSXAttributeItem::SpreadAttribute(_)) {
                self.errors.push(fail(
                    self.source,
                    self.file,
                    item.span(),
                    "translation component cannot use spread attributes",
                ));
                return;
            }
        }

        let Some(text_attr) = get_jsx_text_attribute(&opening.attributes) else {
            self.errors.push(fail(
                self.source,
                self.file,
                opening.span,
                "translation component requires a static text prop",
            ));
            return;
        };

        let texts = match collect_jsx_attribute_strings(
            text_attr,
            self.source,
            self.file,
            "translation text must be a static string",
        ) {
            Ok(texts) => texts,
            Err(error) => {
                self.errors.push(error);
                return;
            }
        };

        let call_notes = if let Some(note_attr) = get_jsx_note_attribute(&opening.attributes) {
            match collect_jsx_attribute_notes(note_attr, self.source, self.file) {
                Ok(notes) => notes,
                Err(error) => {
                    self.errors.push(error);
                    return;
                }
            }
        } else {
            vec![None]
        };

        for text in texts {
            push_encoded_keys(&mut self.keys, &self.hook_symbols, None, &text, &call_notes);
        }
    }
}

impl<'a> Visit<'a> for Compiler<'a> {
    fn visit_import_declaration(&mut self, decl: &ImportDeclaration<'a>) {
        if decl.source.value.as_str() == FUMA_TRANSLATE_REACT {
            if let Some(specifiers) = &decl.specifiers {
                for specifier in specifiers {
                    let ImportDeclarationSpecifier::ImportSpecifier(import) = specifier else {
                        continue;
                    };

                    if import.import_kind == ImportOrExportKind::Type {
                        continue;
                    }

                    let Some(exported) = module_export_name(&import.imported) else {
                        continue;
                    };

                    let Some(kind) = fuma_import_from_export_name(exported) else {
                        continue;
                    };

                    self.fuma_imports.insert(import.local.symbol_id(), kind);
                }
            }
        }

        walk::walk_import_declaration(self, decl);
    }

    fn visit_variable_declarator(&mut self, decl: &VariableDeclarator<'a>) {
        if let Some(init) = &decl.init {
            match parse_translations_hook_call(
                init,
                self.semantic,
                &self.fuma_imports,
                self.source,
                self.file,
            ) {
                Ok(Some(notes)) => {
                    if let BindingPattern::BindingIdentifier(ident) = &decl.id {
                        self.hook_symbols.insert(ident.symbol_id(), notes);
                    }
                }
                Ok(None) => {}
                Err(error) => self.errors.push(error),
            }
        }

        walk::walk_variable_declarator(self, decl);
    }

    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        if let Some(hook_symbol_id) = self.should_analyze_call(call) {
            self.analyze_call(call, hook_symbol_id);
        }

        walk::walk_call_expression(self, call);
    }

    fn visit_jsx_element(&mut self, element: &JSXElement<'a>) {
        if self.should_analyze_jsx(element) {
            self.analyze_jsx(&element.opening_element);
        }

        walk::walk_jsx_element(self, element);
    }
}

fn unwrap_callee<'a>(mut expr: &'a Expression<'a>) -> &'a Expression<'a> {
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
    for note in [hook_note, call_note].into_iter().flatten() {
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
