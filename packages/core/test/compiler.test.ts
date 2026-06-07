import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { compile, StaticAnalysisError } from "../src/compiler.js";

const fixtures = join(dirname(fileURLToPath(import.meta.url)), "fixtures");

function fixture(name: string): string {
  return join(fixtures, name);
}

describe("compile", () => {
  it("extracts encoded keys from static usage", async () => {
    const result = await compile({ input: [fixture("basic.tsx")] });

    expect(result.translationKeys).toEqual([
      "Close(dialog button)",
      "Hello",
      "Hello {user}",
      "Static template",
    ]);
  });

  it("expands conditional note branches", async () => {
    const result = await compile({ input: [fixture("conditional.tsx")] });

    expect(result.translationKeys).toEqual(["Theme(dark mode)", "Theme(light mode)"]);
  });

  it("respects lexical scoping for translation hooks", async () => {
    const result = await compile({ input: [fixture("scoping.tsx")] });

    expect(result.translationKeys).toEqual(["Before block", "From outer scope", "Inside block"]);
  });

  it("ignores calls that are not translation hooks", async () => {
    const result = await compile({ input: [fixture("ignored.tsx")] });

    expect(result.translationKeys).toEqual(["Tracked"]);
  });

  it("merges and deduplicates keys across files", async () => {
    const result = await compile({
      input: [fixture("basic.tsx"), fixture("conditional.tsx")],
    });

    expect(result.translationKeys).toEqual([
      "Close(dialog button)",
      "Hello",
      "Hello {user}",
      "Static template",
      "Theme(dark mode)",
      "Theme(light mode)",
    ]);
  });

  it("writes translations.json when write is enabled", async () => {
    const dir = await mkdtemp(join(tmpdir(), "fuma-translate-"));
    const output = join(dir, "translations.json");

    try {
      await compile({
        input: [fixture("basic.tsx")],
        write: true,
        output,
      });

      const written = JSON.parse(await readFile(output, "utf8"));
      expect(written).toEqual({
        translationKeys: [
          "Close(dialog button)",
          "Hello",
          "Hello {user}",
          "Static template",
        ],
      });
    } finally {
      await rm(dir, { recursive: true });
    }
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
