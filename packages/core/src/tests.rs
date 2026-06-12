use std::path::PathBuf;

use oxc_span::SourceType;

use crate::{analyze_source, join_errors};

fn fixture(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test/fixtures")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn compile_fixture(name: &str) -> Vec<String> {
    let file = fixture(name);
    let source = std::fs::read_to_string(&file).unwrap();
    let source_type = SourceType::from_path(&file).expect("valid fixture extension");
    let analysis = analyze_source(&file, source_type, &source);
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
        compile_fixture("basic.tsx"),
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
        compile_fixture("from-translations.tsx"),
        vec![
            "Dashboard(admin panel)".to_string(),
            "Server Hello".to_string(),
        ]
    );
}

#[test]
fn jsx_fixture() {
    assert_eq!(
        compile_fixture("jsx.tsx"),
        vec![
            "Click <a>here</a> to continue".to_string(),
            "Or <signup/> today(landing page)".to_string(),
        ]
    );
}

#[test]
fn t_component_fixture() {
    assert_eq!(
        compile_fixture("t-component.tsx"),
        vec![
            "Click <a>here</a> to continue".to_string(),
            "Hello {user}".to_string(),
            "Or <signup/> today(landing page)".to_string(),
        ]
    );
}

#[test]
fn strict_ignored_fixture() {
    assert_eq!(
        compile_fixture("ignored.tsx"),
        vec!["From hook".to_string(), "Tracked".to_string()]
    );
}

#[test]
fn renamed_hook_fixture() {
    assert_eq!(
        compile_fixture("renamed-hook.tsx"),
        vec![
            "Hello from myT".to_string(),
            "Read <link>docs</link>".to_string(),
        ]
    );
}

#[test]
fn fake_t_fixture() {
    assert_eq!(compile_fixture("fake-t.tsx"), Vec::<String>::new());
}

#[test]
fn aliased_t_fixture() {
    assert_eq!(
        compile_fixture("aliased-t.tsx"),
        vec!["Aliased hello(sidebar)".to_string()]
    );
}

#[test]
fn annotated_fixture() {
    assert_eq!(
        compile_fixture("annotated.tsx"),
        vec![
            "Annotated call".to_string(),
            "Annotated jsx(sidebar)".to_string(),
            "Block annotated".to_string(),
        ]
    );
}

#[test]
fn directive_with_trailing_comment() {
    let source = r#"
// @ts-expect-error -- faked
import { t } from "<unknown>";

function Label({ text }: { text: string }) {
  return <span>{text}</span>;
}

export function Annotated() {
  // @fuma-translate -- track this call
  t("With reason");

  return (
    // @fuma-translate: sidebar copy
    <Label text="With label" />
  );
}
"#;
    let analysis = analyze_source("directive.tsx", SourceType::tsx(), source);
    assert!(
        analysis.errors.is_empty(),
        "{}",
        join_errors(analysis.errors).message
    );
    let mut keys: Vec<String> = analysis.keys.into_iter().collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["With label".to_string(), "With reason".to_string()]
    );
}

#[test]
fn namespace_imports() {
    let source = r#"
import * as ft from "@fuma-translate/react";

export function NamespaceImport() {
  const t = ft.useTranslations({ note: "namespace hook" });

  return (
    <>
      {t("From namespace")}
      <ft.T text="Namespace component" note="component note" />
    </>
  );
}
"#;
    let analysis = analyze_source("namespace.tsx", SourceType::tsx(), source);
    assert!(
        analysis.errors.is_empty(),
        "{}",
        join_errors(analysis.errors).message
    );
    let mut keys: Vec<String> = analysis.keys.into_iter().collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "From namespace(namespace hook)".to_string(),
            "Namespace component(component note)".to_string(),
        ]
    );
}

#[test]
fn t_alias_member_expression_is_not_component() {
    let source = r#"
import { T as Translate } from "@fuma-translate/react";

export function AliasedMember() {
  return (
    <>
      <Translate text="Real component" />
      <Translate.NotT text="Not a translation component" />
    </>
  );
}
"#;
    let analysis = analyze_source("aliased-member.tsx", SourceType::tsx(), source);
    assert!(
        analysis.errors.is_empty(),
        "{}",
        join_errors(analysis.errors).message
    );
    let mut keys: Vec<String> = analysis.keys.into_iter().collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["Real component".to_string()]);
}

#[test]
fn rejects_non_static_note_accessor() {
    let source = r#"
import { useTranslations } from "@fuma-translate/react";

export function InvalidNote() {
  const t = useTranslations();

  return t("Save", {
    get note() {
      return "dynamic";
    },
  });
}
"#;
    let analysis = analyze_source("invalid-note.tsx", SourceType::tsx(), source);

    assert_eq!(analysis.errors.len(), 1);
    assert!(analysis.errors[0]
        .message
        .contains("translation note must be a static string"));
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
    let analysis = analyze_source("broken.tsx", SourceType::tsx(), source);

    assert_eq!(analysis.errors.len(), 2);
    assert!(analysis.errors.iter().any(|error| error
        .message
        .contains("translation key must be a static string")));
    assert!(analysis.errors.iter().any(|error| error
        .message
        .contains("translation options cannot use spread properties")));
}
