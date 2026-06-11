export interface CompileOptions {
  /** glob patterns */
  input: string[];
  /** when false, extract all static t() calls; defaults to true (only useTranslations/fromTranslations) */
  strict?: boolean;
}

export interface CompileOutput {
  /** All encoded keys */
  translationKeys: string[];
}

export class StaticAnalysisError extends Error {
  constructor(
    message: string,
    readonly file: string = "",
    readonly span?: { start: number; end: number },
  ) {
    super(message);
    this.name = "StaticAnalysisError";
  }
}

export async function compile(options: CompileOptions): Promise<CompileOutput> {
  const { compileSync } = await import("../native/native.js");

  try {
    return compileSync(options.input, options.strict);
  } catch (error) {
    if (error instanceof Error) {
      throw new StaticAnalysisError(error.message);
    }

    throw new StaticAnalysisError(String(error));
  }
}

export function typegen(output: CompileOutput): string {
  if (output.translationKeys.length === 0) {
    return "export type Translations = {};\n";
  }

  const keys = output.translationKeys.sort();
  const entries = keys.map((key) => `  ${JSON.stringify(key)}: string;`).join("\n");

  return `export type Translations = {\n${entries}\n};\n`;
}
