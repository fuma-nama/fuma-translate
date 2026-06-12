#!/usr/bin/env node
import { mkdir, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import chokidar from "chokidar";
import { compile, StaticAnalysisError, typegen, type CompileOutput } from "./compiler";

function writeManifest(output: CompileOutput): string {
  return `${JSON.stringify({ translationKeys: [...output.translationKeys].sort() }, null, 2)}\n`;
}

const HELP = `Usage: fuma-translate [options] <glob>...

Compile translation keys from source files.

Arguments:
  <glob>...              Glob patterns to scan (e.g. src/**/*.tsx)

Options:
  -o, --out <dir>        Output directory (default: .translations)
  -w, --watch            Recompile when matching files change
  -h, --help             Show this help message
`;

export interface CompileAndWriteOptions {
  input: string[];
  out: string;
}

export interface WatchOptions extends CompileAndWriteOptions {
  onCompiled?: (result: { ok: true; keyCount: number } | { ok: false; error: string }) => void;
}

export function getWatchRoots(globs: string[]): string[] {
  const roots = new Set<string>();

  for (const pattern of globs) {
    const index = pattern.search(/[*?[{]/);
    const root =
      index === -1 ? dirname(pattern) : pattern.slice(0, index).replace(/[/\\]+$/, "") || ".";

    roots.add(resolve(root));
  }

  return [...roots];
}

function debounce(fn: () => void, ms: number): () => void {
  let timer: ReturnType<typeof setTimeout> | undefined;

  return () => {
    if (timer) {
      clearTimeout(timer);
    }

    timer = setTimeout(fn, ms);
  };
}

export async function compileAndWrite(
  options: CompileAndWriteOptions,
): Promise<{ keyCount: number }> {
  const output = await compile({ input: options.input });
  const outDir = resolve(options.out);

  await mkdir(outDir, { recursive: true });
  await writeFile(join(outDir, "manifest.json"), writeManifest(output), "utf8");
  await writeFile(join(outDir, "index.ts"), typegen(output), "utf8");

  return { keyCount: output.translationKeys.length };
}

export async function watchCompile(options: WatchOptions, signal?: AbortSignal): Promise<void> {
  const outDir = resolve(options.out);
  const roots = getWatchRoots(options.input);
  let compiling = false;
  let queued = false;

  const run = async () => {
    if (compiling) {
      queued = true;
      return;
    }

    compiling = true;

    try {
      const { keyCount } = await compileAndWrite(options);
      options.onCompiled?.({ ok: true, keyCount });
    } catch (error) {
      const message = error instanceof StaticAnalysisError ? error.message : String(error);

      process.stderr.write(`${message}\n`);
      options.onCompiled?.({ ok: false, error: message });
    } finally {
      compiling = false;

      if (queued) {
        queued = false;
        void run();
      }
    }
  };

  const schedule = debounce(() => {
    void run();
  }, 100);

  const watcher = chokidar.watch(roots, {
    ignoreInitial: true,
    ignored: (path) => path.startsWith(outDir),
    awaitWriteFinish: { stabilityThreshold: 100, pollInterval: 50 },
  });

  watcher.on("all", schedule);

  if (signal) {
    if (signal.aborted) {
      await watcher.close();
      return;
    }

    signal.addEventListener(
      "abort",
      () => {
        void watcher.close();
      },
      { once: true },
    );
  }

  process.stdout.write(`Watching ${options.input.join(", ")}...\n`);
  await run();
}

export async function main(argv: string[] = process.argv.slice(2)): Promise<number> {
  const { values, positionals } = parseArgs({
    args: argv,
    options: {
      out: { type: "string", short: "o", default: ".translations" },
      watch: { type: "boolean", short: "w", default: false },
      help: { type: "boolean", short: "h", default: false },
    },
    allowPositionals: true,
  });

  if (values.help) {
    process.stdout.write(HELP);
    return 0;
  }

  if (positionals.length === 0) {
    process.stderr.write(HELP);
    return 1;
  }

  const options: CompileAndWriteOptions = {
    input: positionals,
    out: values.out,
  };

  if (values.watch) {
    await watchCompile(options);
    return 0;
  }

  try {
    await compileAndWrite(options);
    return 0;
  } catch (error) {
    if (error instanceof StaticAnalysisError) {
      process.stderr.write(`${error.message}\n`);
      return 1;
    }

    throw error;
  }
}

const entry = process.argv[1] ? resolve(process.argv[1]) : "";
const isMain = entry === fileURLToPath(import.meta.url);

if (isMain) {
  main().then((code) => {
    process.exitCode = code;
  });
}
