use oxc_ast::ast::{
    CallExpression, Expression, JSXAttribute, JSXAttributeItem, JSXAttributeValue, PropertyKind,
    TemplateElementValue,
};
use oxc_semantic::Semantic;
use oxc_span::GetSpan;
use oxc_syntax::symbol::SymbolId;
use rustc_hash::FxHashMap;

use crate::error::{fail, join_errors, AnalysisError};

pub(crate) type HookNoteBranches = Vec<Option<String>>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FumaImport {
    UseTranslations,
    FromTranslations,
    T,
    Namespace,
}

type ExprResult<T> = Result<T, AnalysisError>;

pub(crate) fn fuma_import_from_export_name(name: &str) -> Option<FumaImport> {
    match name {
        "useTranslations" => Some(FumaImport::UseTranslations),
        "fromTranslations" => Some(FumaImport::FromTranslations),
        "T" => Some(FumaImport::T),
        _ => None,
    }
}

fn quasi_text(value: &TemplateElementValue<'_>) -> String {
    value
        .cooked
        .map(|cooked| cooked.to_string())
        .unwrap_or_else(|| value.raw.to_string())
}

pub(crate) fn collect_static_strings<'a>(
    expr: &Expression<'a>,
    source: &str,
    file: &str,
) -> ExprResult<Vec<String>> {
    let expr = expr.get_inner_expression();

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

            let value = if template.is_no_substitution_template() {
                quasi_text(&template.quasis[0].value)
            } else {
                template
                    .quasis
                    .iter()
                    .map(|quasi| quasi_text(&quasi.value))
                    .collect()
            };

            Ok(vec![value])
        }
        Expression::ConditionalExpression(conditional) => {
            let mut values = Vec::new();
            let mut errors = Vec::new();

            for branch in [&conditional.consequent, &conditional.alternate] {
                match collect_static_strings(branch, source, file) {
                    Ok(branch_values) => values.extend(branch_values),
                    Err(error) => errors.push(error),
                }
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

pub(crate) fn collect_notes<'a>(
    expr: Option<&Expression<'a>>,
    source: &str,
    file: &str,
) -> ExprResult<HookNoteBranches> {
    let Some(expr) = expr else {
        return Ok(vec![None]);
    };

    let expr = expr.get_inner_expression();

    if let Expression::ConditionalExpression(conditional) = expr {
        let mut notes = Vec::new();
        let mut errors = Vec::new();

        for branch in [&conditional.consequent, &conditional.alternate] {
            match collect_notes(Some(branch), source, file) {
                Ok(branch_notes) => notes.extend(branch_notes),
                Err(error) => errors.push(error),
            }
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
        if prop.is_spread() {
            return Err(fail(
                source,
                file,
                prop.span(),
                "translation options cannot use spread properties",
            ));
        }
    }

    let Some(note_prop) = object.properties.iter().find_map(|prop| {
        let property = prop.as_property()?;
        if !property.key.is_specific_static_name("note") {
            return None;
        }
        Some(property)
    }) else {
        return Ok(vec![None]);
    };

    if note_prop.kind != PropertyKind::Init || note_prop.method || note_prop.shorthand {
        return Err(fail(
            source,
            file,
            note_prop.span,
            "translation note must be a static string",
        ));
    }

    collect_static_strings(&note_prop.value, source, file)
        .map(|notes| notes.into_iter().map(Some).collect())
}

fn parse_use_translations_call<'a>(
    call: &CallExpression<'a>,
    source: &str,
    file: &str,
) -> ExprResult<HookNoteBranches> {
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
) -> ExprResult<HookNoteBranches> {
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

pub(crate) fn parse_translations_hook_call<'a>(
    expr: &Expression<'a>,
    semantic: &Semantic<'a>,
    fuma_imports: &FxHashMap<SymbolId, FumaImport>,
    source: &str,
    file: &str,
) -> ExprResult<Option<HookNoteBranches>> {
    let Expression::CallExpression(call) = expr.get_inner_expression() else {
        return Ok(None);
    };

    let Some(import) = call_callee_import(call, semantic, fuma_imports) else {
        return Ok(None);
    };

    match import {
        FumaImport::UseTranslations => parse_use_translations_call(call, source, file).map(Some),
        FumaImport::FromTranslations => parse_from_translations_call(call, source, file).map(Some),
        _ => Ok(None),
    }
}

fn identifier_import<'a>(
    ident: &oxc_ast::ast::IdentifierReference<'a>,
    semantic: &Semantic<'a>,
    fuma_imports: &FxHashMap<SymbolId, FumaImport>,
) -> Option<FumaImport> {
    let reference = semantic.scoping().get_reference(ident.reference_id());
    let symbol_id = reference.symbol_id()?;

    fuma_imports.get(&symbol_id).copied()
}

fn call_callee_import<'a>(
    call: &CallExpression<'a>,
    semantic: &Semantic<'a>,
    fuma_imports: &FxHashMap<SymbolId, FumaImport>,
) -> Option<FumaImport> {
    let callee = call.callee.get_inner_expression();

    if let Expression::Identifier(ident) = callee {
        return identifier_import(ident, semantic, fuma_imports);
    }

    let member = callee.as_member_expression()?;
    let import = fuma_import_from_export_name(member.static_property_name()?)?;
    let Expression::Identifier(object) = member.object().get_inner_expression() else {
        return None;
    };

    (identifier_import(object, semantic, fuma_imports) == Some(FumaImport::Namespace))
        .then_some(import)
}

pub(crate) fn find_jsx_attribute<'a>(
    attributes: &'a [JSXAttributeItem<'a>],
    name: &str,
) -> Option<&'a JSXAttribute<'a>> {
    attributes
        .iter()
        .filter_map(JSXAttributeItem::as_attribute)
        .find(|attr| attr.is_identifier(name))
}

pub(crate) fn collect_jsx_attribute_strings<'a>(
    attr: &JSXAttribute<'a>,
    source: &str,
    file: &str,
    message: &str,
) -> ExprResult<Vec<String>> {
    let Some(value) = &attr.value else {
        return Err(fail(source, file, attr.span, message));
    };

    match value {
        JSXAttributeValue::StringLiteral(literal) => Ok(vec![literal.value.to_string()]),
        JSXAttributeValue::ExpressionContainer(container) => {
            let Some(expr) = container.expression.as_expression() else {
                return Err(fail(source, file, container.span, message));
            };

            collect_static_strings(expr, source, file)
        }
        _ => Err(fail(source, file, value.span(), message)),
    }
}

pub(crate) fn collect_jsx_attribute_notes<'a>(
    attr: &JSXAttribute<'a>,
    source: &str,
    file: &str,
) -> ExprResult<HookNoteBranches> {
    const MESSAGE: &str = "translation note must be a static string";

    let Some(value) = &attr.value else {
        return Err(fail(source, file, attr.span, MESSAGE));
    };

    match value {
        JSXAttributeValue::StringLiteral(literal) => Ok(vec![Some(literal.value.to_string())]),
        JSXAttributeValue::ExpressionContainer(container) => {
            let Some(expr) = container.expression.as_expression() else {
                return Err(fail(source, file, container.span, MESSAGE));
            };

            collect_notes(Some(expr), source, file)
        }
        _ => Err(fail(source, file, value.span(), MESSAGE)),
    }
}
