import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { describe, expect, it, vi } from "vitest";
import { getWatchRoots, main, watchCompile } from "../src/cli.js";

const fixtures = join(import.meta.dirname, "fixtures");

describe("getWatchRoots", () => {
  it("derives directories from glob patterns", () => {
    expect(getWatchRoots(["src/**/*.tsx"])).toEqual([resolve("src")]);
    expect(getWatchRoots(["**/*.tsx"])).toEqual([resolve(".")]);
    expect(getWatchRoots(["src/a/*.tsx", "lib/**/*.ts"])).toEqual([
      resolve("src/a"),
      resolve("lib"),
    ]);
  });

  it("uses the file directory for literal paths", () => {
    expect(getWatchRoots(["test/fixtures/basic.tsx"])).toEqual([resolve("test/fixtures")]);
  });
});

describe("cli", () => {
  it("writes manifest.json and index.ts to the output directory", async () => {
    const outDir = await mkdtemp(join(tmpdir(), "fuma-translate-cli-"));

    try {
      const code = await main(["--out", outDir, join(fixtures, "basic.tsx")]);

      expect(code).toBe(0);
      expect(await readFile(join(outDir, "manifest.json"), "utf8")).toMatchInlineSnapshot(`
        "{
          "translationKeys": [
            "Close(dialog button)",
            "Hello",
            "Hello {user}",
            "Static template"
          ]
        }
        "
      `);
      expect(await readFile(join(outDir, "index.ts"), "utf8")).toMatchInlineSnapshot(`
        "export type Translations = {
          "Close(dialog button)": string;
          "Hello": string;
          "Hello {user}": string;
          "Static template": string;
        };
        "
      `);
    } finally {
      await rm(outDir, { recursive: true, force: true });
    }
  });

  it("defaults the output directory to .translations", async () => {
    const cwd = await mkdtemp(join(tmpdir(), "fuma-translate-cwd-"));
    const previousCwd = process.cwd();

    try {
      process.chdir(cwd);
      const code = await main([join(fixtures, "basic.tsx")]);

      expect(code).toBe(0);
      expect(await readFile(join(cwd, ".translations", "manifest.json"), "utf8")).toContain(
        '"Hello"',
      );
    } finally {
      process.chdir(previousCwd);
      await rm(cwd, { recursive: true, force: true });
    }
  });

  it("returns a non-zero exit code on compile errors", async () => {
    const outDir = await mkdtemp(join(tmpdir(), "fuma-translate-cli-error-"));

    try {
      const code = await main(["--out", outDir, join(fixtures, "dynamic-key.tsx")]);

      expect(code).toBe(1);
    } finally {
      await rm(outDir, { recursive: true, force: true });
    }
  });

  it("compiles files matched by glob patterns", async () => {
    const outDir = await mkdtemp(join(tmpdir(), "fuma-translate-cli-glob-"));

    try {
      const code = await main([
        "--out",
        outDir,
        join(fixtures, "b*.tsx"),
        join(fixtures, "conditional.tsx"),
      ]);

      expect(code).toBe(0);
      expect(await readFile(join(outDir, "manifest.json"), "utf8")).toContain('"Hello"');
      expect(await readFile(join(outDir, "manifest.json"), "utf8")).toContain('"Theme(dark mode)"');
    } finally {
      await rm(outDir, { recursive: true, force: true });
    }
  });

  it("recompiles when watched files change", async () => {
    const root = await mkdtemp(join(tmpdir(), "fuma-translate-watch-"));
    const srcDir = join(root, "src");
    const outDir = join(root, ".translations");
    const source = join(srcDir, "app.tsx");
    const controller = new AbortController();
    const compiled: Array<{ ok: boolean; keyCount?: number }> = [];

    try {
      await mkdir(srcDir, { recursive: true });
      await writeFile(
        source,
        `import { useTranslations } from "@fuma-translate/react";
export function App() {
  const t = useTranslations();
  return <p>{t("Hello")}</p>;
}`,
        "utf8",
      );

      await watchCompile(
        {
          input: [join(srcDir, "**/*.tsx")],
          out: outDir,
          strict: true,
          onCompiled: (result) => {
            compiled.push(result.ok ? { ok: true, keyCount: result.keyCount } : { ok: false });
          },
        },
        controller.signal,
      );

      await vi.waitFor(() => {
        expect(compiled).toHaveLength(1);
      });

      expect(compiled[0]).toEqual({ ok: true, keyCount: 1 });
      expect(await readFile(join(outDir, "manifest.json"), "utf8")).toContain('"Hello"');

      await writeFile(
        source,
        `import { useTranslations } from "@fuma-translate/react";
export function App() {
  const t = useTranslations();
  return <p>{t("Hello")}{t("Goodbye")}</p>;
}`,
        "utf8",
      );

      await vi.waitFor(
        () => {
          expect(compiled).toHaveLength(2);
        },
        { timeout: 3000 },
      );

      expect(compiled[1]).toEqual({ ok: true, keyCount: 2 });
      expect(await readFile(join(outDir, "manifest.json"), "utf8")).toContain('"Goodbye"');

      controller.abort();
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });
});
