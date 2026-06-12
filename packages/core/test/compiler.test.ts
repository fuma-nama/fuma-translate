import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { compile, StaticAnalysisError, typegen } from "../src/compiler.js";

const fixtures = join(dirname(fileURLToPath(import.meta.url)), "fixtures");

function fixture(name: string): string {
  return join(fixtures, name);
}

function sortedKeys(keys: string[]): string[] {
  return keys.sort();
}

describe("compile", () => {
  it("extracts encoded keys from static usage", async () => {
    const result = await compile({ input: [fixture("basic.tsx")] });

    expect(sortedKeys(result.translationKeys)).toEqual([
      "Close(dialog button)",
      "Hello",
      "Hello {user}",
      "Static template",
    ]);
  });

  it("preserves backslash-escaped braces as literal placeholders", async () => {
    const result = await compile({ input: [fixture("escaped-variables.tsx")] });

    expect(sortedKeys(result.translationKeys)).toMatchInlineSnapshot(`
      [
        "Hello {user}",
        "Show \\{literal} braces {var}",
      ]
    `);
  });

  it("expands conditional note branches", async () => {
    const result = await compile({ input: [fixture("conditional.tsx")] });

    expect(sortedKeys(result.translationKeys)).toEqual(["Theme(dark mode)", "Theme(light mode)"]);
  });

  it("combines hook-level and call-level notes", async () => {
    const result = await compile({ input: [fixture("hook-note.tsx")] });

    expect(sortedKeys(result.translationKeys)).toEqual([
      "Cancel(settings page)(dialog button)",
      "Save(settings page)",
      "Theme(dark mode)",
      "Theme(light mode)",
    ]);
  });

  it("respects lexical scoping for translation hooks", async () => {
    const result = await compile({ input: [fixture("scoping.tsx")] });

    expect(sortedKeys(result.translationKeys)).toEqual([
      "Before block",
      "From outer scope",
      "Inside block",
    ]);
  });

  it("extracts valid t() calls and ignores invalid non-hook calls", async () => {
    const result = await compile({ input: [fixture("ignored.tsx")] });

    expect(sortedKeys(result.translationKeys)).toEqual(["From hook", "Tracked"]);
  });

  it("extracts calls and JSX annotated with // @fuma-translate", async () => {
    const result = await compile({ input: [fixture("annotated.tsx")] });

    expect(sortedKeys(result.translationKeys)).toEqual([
      "Annotated call",
      "Annotated jsx(sidebar)",
      "Block annotated",
    ]);
  });

  it("extracts keys from renamed translation hooks", async () => {
    const result = await compile({ input: [fixture("renamed-hook.tsx")] });

    expect(sortedKeys(result.translationKeys)).toEqual([
      "Hello from myT",
      "Read <link>docs</link>",
    ]);
  });

  it("ignores local T components not imported from @fuma-translate/react", async () => {
    const result = await compile({ input: [fixture("fake-t.tsx")] });

    expect(sortedKeys(result.translationKeys)).toEqual([]);
  });

  it("extracts keys from aliased T imports", async () => {
    const result = await compile({ input: [fixture("aliased-t.tsx")] });

    expect(sortedKeys(result.translationKeys)).toEqual(["Aliased hello(sidebar)"]);
  });

  it("extracts keys from fromTranslations()", async () => {
    const result = await compile({ input: [fixture("from-translations.tsx")] });

    expect(sortedKeys(result.translationKeys)).toEqual(["Dashboard(admin panel)", "Server Hello"]);
  });

  it("extracts keys from t.jsx()", async () => {
    const result = await compile({ input: [fixture("jsx.tsx")] });

    expect(sortedKeys(result.translationKeys)).toEqual([
      "Click <a>here</a> to continue",
      "Or <signup/> today(landing page)",
    ]);
  });

  it("extracts keys from <T />", async () => {
    const result = await compile({ input: [fixture("t-component.tsx")] });

    expect(sortedKeys(result.translationKeys)).toEqual([
      "Click <a>here</a> to continue",
      "Hello {user}",
      "Or <signup/> today(landing page)",
    ]);
  });

  it("merges and deduplicates keys across files", async () => {
    const result = await compile({
      input: [fixture("basic.tsx"), fixture("conditional.tsx")],
    });

    expect(sortedKeys(result.translationKeys)).toEqual([
      "Close(dialog button)",
      "Hello",
      "Hello {user}",
      "Static template",
      "Theme(dark mode)",
      "Theme(light mode)",
    ]);
  });

  it("reports all analysis errors across files", async () => {
    await expect(
      compile({ input: [fixture("dynamic-key.tsx"), fixture("spread-options.tsx")] }),
    ).rejects.toSatisfy((error: unknown) => {
      if (!(error instanceof StaticAnalysisError)) {
        return false;
      }

      return (
        error.message.includes("translation key must be a static string") &&
        error.message.includes("translation options cannot use spread properties")
      );
    });
  });

  it("throws when translation options are invalid on a useTranslations hook", async () => {
    await expect(compile({ input: [fixture("invalid-hook-call.tsx")] })).rejects.toSatisfy(
      (error: unknown) =>
        error instanceof StaticAnalysisError &&
        error.message.includes("translation options must be a static object"),
    );
  });

  it("throws when the translation key is dynamic", async () => {
    await expect(compile({ input: [fixture("dynamic-key.tsx")] })).rejects.toSatisfy(
      (error: unknown) =>
        error instanceof StaticAnalysisError &&
        error.message.includes("translation key must be a static string"),
    );
  });

  it("throws when the translation key uses template interpolation", async () => {
    await expect(compile({ input: [fixture("dynamic-template.tsx")] })).rejects.toSatisfy(
      (error: unknown) =>
        error instanceof StaticAnalysisError &&
        error.message.includes("translation key must be a static string"),
    );
  });

  it("throws when translation options use spread", async () => {
    await expect(compile({ input: [fixture("spread-options.tsx")] })).rejects.toSatisfy(
      (error: unknown) =>
        error instanceof StaticAnalysisError &&
        error.message.includes("translation options cannot use spread properties"),
    );
  });
});

describe("typegen", () => {
  it("generates a Translations object type from compile output", async () => {
    const output = await compile({ input: [fixture("basic.tsx")] });

    expect(typegen(output)).toBe(`export type Translations = {
  "Close(dialog button)": string;
  "Hello": string;
  "Hello {user}": string;
  "Static template": string;
};
`);
  });

  it("generates an empty Translations type when there are no keys", () => {
    expect(typegen({ translationKeys: [] })).toBe("export type Translations = {};\n");
  });
});
