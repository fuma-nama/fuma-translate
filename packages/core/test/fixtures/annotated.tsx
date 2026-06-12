// @ts-expect-error -- faked
import { t } from "<unknown>";

function Label({ text, note }: { text: string; note?: string }) {
  return <span data-note={note}>{text}</span>;
}

export function AnnotatedCall() {
  // @fuma-translate
  t("Annotated call");
}

export function AnnotatedJsx() {
  return (
    // @fuma-translate
    <Label text="Annotated jsx" note="sidebar" />
  );
}

export function AnnotatedJsxBlock() {
  return (
    <>
      {/* @fuma-translate */}
      <Label text="Block annotated" />
    </>
  );
}
