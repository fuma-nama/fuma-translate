import fs from "node:fs/promises";
import path from "node:path";
import type {
  BindingPattern,
  CallExpression,
  Expression,
  ObjectProperty,
  ObjectPropertyKind,
  ParamPattern,
  Span,
  Statement,
} from "oxc-parser";
import { parseSync, Visitor } from "oxc-parser";
import { glob } from "tinyglobby";

type SupportedLang = "js" | "jsx" | "ts" | "tsx";

export interface CompileOptions {
  /** glob patterns */
  input: string[];
  /** when true, only extract t() calls bound to useTranslations() */
  strict?: boolean;
}

export interface CompileOutput {
  /** All encoded keys */
  translationKeys: string[];
}

export class StaticAnalysisError extends Error {
  constructor(
    message: string,
    readonly file: string,
    readonly span?: Span,
  ) {
    super(message);
    this.name = "StaticAnalysisError";
  }
}

function formatLocation(source: string, offset: number): string {
  let line = 1;
  let column = 1;

  for (let i = 0; i < offset && i < source.length; i++) {
    if (source[i] === "\n") {
      line++;
      column = 1;
    } else {
      column++;
    }
  }

  return `${line}:${column}`;
}

type HookNoteBranches = (string | undefined)[];

function parseUseTranslationsCall(
  expr: Expression,
  source: string,
  file: string,
): HookNoteBranches | null {
  if (expr.type !== "CallExpression") return null;
  if (expr.callee.type !== "Identifier" || expr.callee.name !== "useTranslations") {
    return null;
  }

  if (expr.arguments.length === 0) return [undefined];

  if (expr.arguments.length > 1) {
    fail(source, file, expr, "useTranslations accepts at most one options argument");
  }

  const arg = expr.arguments[0];
  if (!arg || arg.type === "SpreadElement") {
    fail(source, file, arg ?? expr, "useTranslations options must be a static object");
  }

  return collectNotes(arg, source, file);
}

function unwrapExpression(expr: Expression): Expression {
  while (
    expr.type === "ParenthesizedExpression" ||
    expr.type === "TSAsExpression" ||
    expr.type === "TSSatisfiesExpression" ||
    expr.type === "TSTypeAssertion"
  ) {
    expr = expr.expression;
  }
  return expr;
}

function fail(source: string, file: string, span: Span, message: string): never {
  throw new StaticAnalysisError(
    `${file}:${formatLocation(source, span.start)}: ${message}`,
    file,
    span,
  );
}

function collectStaticStrings(expr: Expression, source: string, file: string): string[] {
  expr = unwrapExpression(expr);

  if (expr.type === "Literal" && typeof expr.value === "string") {
    return [expr.value];
  }

  if (expr.type === "TemplateLiteral") {
    if (expr.expressions.length > 0) {
      fail(source, file, expr, "translation key must be a static string");
    }
    return [expr.quasis.map((q) => q.value.cooked ?? q.value.raw).join("")];
  }

  if (expr.type === "ConditionalExpression") {
    return [
      ...collectStaticStrings(expr.consequent, source, file),
      ...collectStaticStrings(expr.alternate, source, file),
    ];
  }

  fail(source, file, expr, "translation key must be a static string");
}

function getNoteProperty(properties: ObjectPropertyKind[]): ObjectProperty | undefined {
  for (const prop of properties) {
    if (prop.type !== "Property") continue;
    if (prop.kind !== "init") continue;

    if (prop.shorthand && prop.key.type === "Identifier") {
      if (prop.key.name === "note") return prop;
      continue;
    }

    if (prop.key.type === "Identifier" && prop.key.name === "note") {
      return prop;
    }

    if (
      prop.key.type === "Literal" &&
      typeof prop.key.value === "string" &&
      prop.key.value === "note"
    ) {
      return prop;
    }
  }
  return undefined;
}

function collectNotes(
  expr: Expression | undefined,
  source: string,
  file: string,
): (string | undefined)[] {
  if (!expr) return [undefined];

  expr = unwrapExpression(expr);

  if (expr.type === "ConditionalExpression") {
    return [
      ...collectNotes(expr.consequent, source, file),
      ...collectNotes(expr.alternate, source, file),
    ];
  }

  if (expr.type !== "ObjectExpression") {
    fail(source, file, expr, "translation options must be a static object");
  }

  for (const prop of expr.properties) {
    if (prop.type === "SpreadElement") {
      fail(source, file, prop, "translation options cannot use spread properties");
    }
  }

  const noteProp = getNoteProperty(expr.properties);
  if (!noteProp) return [undefined];

  if (noteProp.shorthand) {
    fail(source, file, noteProp, "translation note must be a static string");
  }

  const noteValues = collectStaticStrings(noteProp.value, source, file);
  return noteValues.map((note) => note);
}

function currentScope(
  scopes: Map<string, HookNoteBranches | false>[],
): Map<string, HookNoteBranches | false> {
  const scope = scopes.at(-1);
  if (!scope) throw new Error("scope stack is empty");
  return scope;
}

function registerBinding(
  pattern: BindingPattern,
  hookNotes: HookNoteBranches | false,
  scopes: Map<string, HookNoteBranches | false>[],
): void {
  switch (pattern.type) {
    case "Identifier":
      currentScope(scopes).set(pattern.name, hookNotes);
      return;
    case "ObjectPattern":
      for (const prop of pattern.properties) {
        if (prop.type === "RestElement") {
          registerBinding(prop.argument, false, scopes);
        } else {
          registerBinding(prop.value, false, scopes);
        }
      }
      return;
    case "ArrayPattern":
      for (const element of pattern.elements) {
        if (!element) continue;
        if (element.type === "RestElement") {
          registerBinding(element.argument, false, scopes);
        } else {
          registerBinding(element, false, scopes);
        }
      }
      return;
    case "AssignmentPattern":
      registerBinding(pattern.left, hookNotes, scopes);
      return;
  }
}

function registerParams(
  params: ParamPattern[],
  scopes: Map<string, HookNoteBranches | false>[],
): void {
  for (const param of params) {
    if (param.type === "TSParameterProperty") {
      registerBinding(param.parameter, false, scopes);
    } else if (param.type === "RestElement") {
      registerBinding(param.argument, false, scopes);
    } else {
      registerBinding(param, false, scopes);
    }
  }
}

function getTranslationHookNotes(
  name: string,
  scopes: Map<string, HookNoteBranches | false>[],
): HookNoteBranches | false {
  for (let i = scopes.length - 1; i >= 0; i--) {
    const scope = scopes[i];
    if (!scope) continue;

    const b = scope.get(name);
    if (b !== undefined) return b;
  }
  return false;
}

function encodeTranslationKey(
  text: string,
  hookNotes: HookNoteBranches,
  callNotes: HookNoteBranches,
): string[] {
  const keys: string[] = [];

  for (const hookNote of hookNotes) {
    for (const callNote of callNotes) {
      const notes: string[] = [];
      if (hookNote) notes.push(hookNote);
      if (callNote) notes.push(callNote);
      keys.push(encodeKey(text, notes));
    }
  }

  return keys;
}

function analyzeCall(
  call: CallExpression,
  source: string,
  file: string,
  keys: Set<string>,
  scopes: Map<string, HookNoteBranches | false>[],
  strict: boolean,
): void {
  const callee = unwrapExpression(call.callee);
  if (callee.type !== "Identifier" || callee.name !== "t") return;

  const hookNotes = getTranslationHookNotes(callee.name, scopes);
  const fromHook = hookNotes !== false;
  if (strict && !fromHook) return;

  const effectiveHookNotes: HookNoteBranches = fromHook ? hookNotes : [undefined];

  try {
    if (call.arguments.length === 0) {
      fail(source, file, call, "translation call requires a static string argument");
    }

    if (call.arguments.length > 2) {
      fail(source, file, call, "translation call accepts at most two arguments");
    }

    const firstArg = call.arguments[0];
    if (!firstArg || firstArg.type === "SpreadElement") {
      fail(source, file, firstArg ?? call, "translation key must be a static string");
    }

    const texts = collectStaticStrings(firstArg, source, file);
    let callNotes: HookNoteBranches = [undefined];
    if (call.arguments.length > 1) {
      const secondArg = call.arguments[1];
      if (!secondArg || secondArg.type === "SpreadElement") {
        fail(source, file, secondArg ?? call, "translation options must be a static object");
      }

      callNotes = collectNotes(secondArg, source, file);
    }

    for (const text of texts) {
      for (const key of encodeTranslationKey(text, effectiveHookNotes, callNotes)) {
        keys.add(key);
      }
    }
  } catch (e) {
    if (!fromHook && e instanceof StaticAnalysisError) return;
    throw e;
  }
}

function analyzeSource(
  file: string,
  lang: SupportedLang,
  source: string,
  strict: boolean,
): string[] {
  const result = parseSync(file, source, { lang, sourceType: "module" });

  if (result.errors.length > 0) {
    const message = result.errors.map((error) => error.message).join("\n");
    throw new StaticAnalysisError(message, file);
  }

  const keys = new Set<string>();
  const scopes: Map<string, HookNoteBranches | false>[] = [new Map()];

  const pushScope = (statements?: Statement[]) => {
    scopes.push(new Map());
    if (statements)
      for (const stmt of statements) {
        if (stmt.type !== "FunctionDeclaration" || !stmt.id) continue;
        registerBinding(stmt.id, false, scopes);
      }
  };

  const popScope = () => {
    scopes.pop();
  };

  const visitor = new Visitor({
    BlockStatement(node) {
      pushScope(node.body);
    },
    "BlockStatement:exit": popScope,

    CatchClause() {
      pushScope();
    },
    "CatchClause:exit": popScope,

    FunctionDeclaration(node) {
      pushScope();
      registerParams(node.params, scopes);
    },
    "FunctionDeclaration:exit": popScope,

    FunctionExpression(node) {
      pushScope();
      registerParams(node.params, scopes);
    },
    "FunctionExpression:exit": popScope,

    ArrowFunctionExpression(node) {
      pushScope();
      registerParams(node.params, scopes);
    },
    "ArrowFunctionExpression:exit": popScope,

    VariableDeclarator(decl) {
      if (!decl.init) return;

      const init = unwrapExpression(decl.init);
      const hookNotes = parseUseTranslationsCall(init, source, file);
      registerBinding(decl.id, hookNotes ?? false, scopes);
    },

    CallExpression(call) {
      analyzeCall(call, source, file, keys, scopes, strict);
    },
  });

  visitor.visit(result.program);
  return [...keys];
}

function getLang(file: string): SupportedLang | undefined {
  switch (path.extname(file)) {
    case ".tsx":
      return "tsx";
    case ".ts":
    case ".cts":
    case ".mts":
      return "ts";
    case ".jsx":
      return "jsx";
    case ".cjs":
    case ".mjs":
    case ".js":
      return "js";
  }
}

export async function compile(options: CompileOptions): Promise<CompileOutput> {
  const files = await glob(options.input, { absolute: true });
  const keys = new Set<string>();

  for (const file of files) {
    const lang = getLang(file);
    if (!lang) continue;

    const source = await fs.readFile(file, "utf8");
    for (const key of analyzeSource(file, lang, source, options.strict ?? false)) {
      keys.add(key);
    }
  }

  const translationKeys = [...keys].sort();
  return { translationKeys };
}

export function typegen(output: CompileOutput): string {
  if (output.translationKeys.length === 0) {
    return "export type Translations = {};\n";
  }

  const entries = output.translationKeys
    .map((key) => `  ${JSON.stringify(key)}: string;`)
    .join("\n");

  return `export type Translations = {\n${entries}\n};\n`;
}

function encodeKey(text: string, notes: string[]): string {
  return text + notes.map((n) => `(${n})`).join("");
}
