import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, extname, resolve } from "node:path";
import type {
  BindingPattern,
  CallExpression,
  Expression,
  ObjectProperty,
  ObjectPropertyKind,
  ParamPattern,
  Span,
} from "oxc-parser";
import { parseSync, Visitor } from "oxc-parser";
import { glob } from "tinyglobby";
import { encodeKey } from "./shared";

export interface CompileOptions {
  /** glob patterns */
  input: string[];

  /** write output to files */
  write?: boolean;

  /** output file path, defaults to `translations.json` */
  output?: string;
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

function isUseTranslationsCall(expr: Expression): boolean {
  return (
    expr.type === "CallExpression" &&
    expr.callee.type === "Identifier" &&
    expr.callee.name === "useTranslations" &&
    expr.arguments.length === 0
  );
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

function currentScope(scopes: Map<string, boolean>[]): Map<string, boolean> {
  const scope = scopes.at(-1);
  if (!scope) throw new Error("scope stack is empty");
  return scope;
}

function registerBinding(
  pattern: BindingPattern,
  isHook: boolean,
  scopes: Map<string, boolean>[],
): void {
  switch (pattern.type) {
    case "Identifier":
      currentScope(scopes).set(pattern.name, isHook);
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
      registerBinding(pattern.left, isHook, scopes);
      return;
  }
}

function registerParams(params: ParamPattern[], scopes: Map<string, boolean>[]): void {
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

function isTranslationHook(name: string, scopes: Map<string, boolean>[]): boolean {
  for (let i = scopes.length - 1; i >= 0; i--) {
    const scope = scopes[i];
    if (!scope) continue;
    if (scope.has(name)) return scope.get(name)!;
  }
  return false;
}

function analyzeCall(
  call: CallExpression,
  source: string,
  file: string,
  keys: Set<string>,
  scopes: Map<string, boolean>[],
): void {
  const callee = unwrapExpression(call.callee);
  if (callee.type !== "Identifier") return;
  if (!isTranslationHook(callee.name, scopes)) return;

  if (call.arguments.length === 0) {
    fail(source, file, call, "translation call requires a static string argument");
  }

  const firstArg = call.arguments[0];
  if (!firstArg || firstArg.type === "SpreadElement") {
    fail(source, file, firstArg ?? call, "translation key must be a static string");
  }

  const texts = collectStaticStrings(firstArg, source, file);

  let notes: (string | undefined)[] = [undefined];
  if (call.arguments.length > 1) {
    const secondArg = call.arguments[1];
    if (!secondArg || secondArg.type === "SpreadElement") {
      fail(source, file, secondArg ?? call, "translation options must be a static object");
    }
    notes = collectNotes(secondArg, source, file);
  }

  for (const text of texts) {
    for (const note of notes) {
      keys.add(encodeKey(text, note));
    }
  }
}

function analyzeSource(file: string, source: string): string[] {
  const lang = getLang(file);
  const result = parseSync(file, source, { lang, sourceType: "module" });

  if (result.errors.length > 0) {
    const message = result.errors.map((error) => error.message).join("\n");
    throw new StaticAnalysisError(message, file);
  }

  const keys = new Set<string>();
  const scopes: Map<string, boolean>[] = [new Map()];

  const pushScope = () => {
    scopes.push(new Map());
  };

  const popScope = () => {
    scopes.pop();
  };

  const visitor = new Visitor({
    BlockStatement: pushScope,
    "BlockStatement:exit": popScope,

    CatchClause: pushScope,
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
      const isHook = init.type === "CallExpression" && isUseTranslationsCall(init);
      registerBinding(decl.id, isHook, scopes);
    },

    CallExpression(call) {
      analyzeCall(call, source, file, keys, scopes);
    },
  });

  visitor.visit(result.program);
  return [...keys];
}

function getLang(file: string): "js" | "jsx" | "ts" | "tsx" {
  switch (extname(file)) {
    case ".tsx":
      return "tsx";
    case ".ts":
      return "ts";
    case ".jsx":
      return "jsx";
    default:
      return "js";
  }
}

export async function compile(options: CompileOptions): Promise<CompileOutput> {
  const files = await glob(options.input, { absolute: true });
  const keys = new Set<string>();

  for (const file of files) {
    const source = await readFile(file, "utf8");
    for (const key of analyzeSource(file, source)) {
      keys.add(key);
    }
  }

  const translationKeys = [...keys].sort();
  const output: CompileOutput = { translationKeys };

  if (options.write) {
    const file = resolve(options.output ?? "translations.json");
    await mkdir(dirname(file), { recursive: true });
    await writeFile(file, `${JSON.stringify(output, null, 2)}\n`, "utf8");
  }

  return output;
}
