use oxc_ast::ast::{
    CallExpression, Expression, JSXAttribute, JSXAttributeItem, JSXAttributeName,
    JSXAttributeValue, ModuleExportName, ObjectPropertyKind, PropertyKey,
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

pub(crate) fn module_export_name<'a>(imported: &'a ModuleExportName<'a>) -> Option<&'a str> {
    match imported {
        ModuleExportName::IdentifierName(ident) => Some(ident.name.as_str()),
        ModuleExportName::IdentifierReference(ident) => Some(ident.name.as_str()),
        ModuleExportName::StringLiteral(literal) => Some(literal.value.as_str()),
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

pub(crate) fn collect_static_strings<'a>(
    expr: &Expression<'a>,
    source: &str,
    file: &str,
) -> ExprResult<Vec<String>> {
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

            let value = if template.quasis.len() == 1 {
                let quasi = &template.quasis[0];
                quasi
                    .value
                    .cooked
                    .as_ref()
                    .map_or_else(|| quasi.value.raw.to_string(), ToString::to_string)
            } else {
                template
                    .quasis
                    .iter()
                    .map(|quasi| {
                        quasi
                            .value
                            .cooked
                            .as_ref()
                            .map_or_else(|| quasi.value.raw.as_str(), AsRef::as_ref)
                    })
                    .collect::<String>()
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

pub(crate) fn collect_notes<'a>(
    expr: Option<&Expression<'a>>,
    source: &str,
    file: &str,
) -> ExprResult<HookNoteBranches> {
    let Some(expr) = expr else {
        return Ok(vec![None]);
    };

    let expr = unwrap_expression(expr);

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
    let Expression::CallExpression(call) = unwrap_expression(expr) else {
        return Ok(None);
    };

    let Expression::Identifier(callee) = unwrap_expression(&call.callee) else {
        return Ok(None);
    };

    let reference = semantic.scoping().get_reference(callee.reference_id());
    let Some(symbol_id) = reference.symbol_id() else {
        return Ok(None);
    };

    match fuma_imports.get(&symbol_id) {
        Some(FumaImport::UseTranslations) => parse_use_translations_call(call, source, file).map(Some),
        Some(FumaImport::FromTranslations) => parse_from_translations_call(call, source, file).map(Some),
        _ => Ok(None),
    }
}

fn get_jsx_attribute<'a>(
    attributes: &'a [JSXAttributeItem<'a>],
    name: &str,
) -> Option<&'a JSXAttribute<'a>> {
    attributes.iter().find_map(|item| {
        let JSXAttributeItem::Attribute(attr) = item else {
            return None;
        };

        match &attr.name {
            JSXAttributeName::Identifier(ident) if ident.name == name => Some(&**attr),
            _ => None,
        }
    })
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

pub(crate) fn get_jsx_text_attribute<'a>(
    attributes: &'a [JSXAttributeItem<'a>],
) -> Option<&'a JSXAttribute<'a>> {
    get_jsx_attribute(attributes, "text")
}

pub(crate) fn get_jsx_note_attribute<'a>(
    attributes: &'a [JSXAttributeItem<'a>],
) -> Option<&'a JSXAttribute<'a>> {
    get_jsx_attribute(attributes, "note")
}
