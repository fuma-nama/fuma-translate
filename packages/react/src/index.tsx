"use client";
import { cloneElement, createContext, isValidElement, use, useMemo, type ReactNode } from "react";

const Context = createContext<Partial<Record<string, string>>>({});

/** add translations, you can stack multiple <TranslationProvider /> to override/extend translations */
export function TranslationProvider({
  translations,
  children,
}: {
  translations: Partial<Record<string, string>>;
  children: ReactNode;
}) {
  const parent = use(Context);
  return (
    <Context value={useMemo(() => ({ ...parent, ...translations }), [parent, translations])}>
      {children}
    </Context>
  );
}

type GetVariables<T extends string> = T extends `${infer Before}{${infer K}}${infer After}`
  ? (Before extends `${string}\\` ? never : K) | GetVariables<After>
  : never;

type GetTags<T extends string> = T extends `${infer Before}<${infer K}>${infer After}`
  ?
      | (Before extends `${string}\\`
          ? never
          : K extends `/${string}`
            ? never
            : K extends `${infer Tag}/`
              ? Tag
              : K)
      | GetTags<After>
  : never;

interface HookOptions {
  /** provide additional context to all t() calls */
  note?: string;
}

export interface TranslationsHook {
  translations: Partial<Record<string, string>>;

  <Text extends string>(
    text: Text,
    opts?: {
      /**
       * add more context to `text`.
       * @example "The aria-label of close dialog button"
       */
      note?: string;
      variables?: Record<GetVariables<Text>, string>;
    },
  ): string;

  jsx<Text extends string>(
    text: Text,
    opts?: {
      /**
       * add more context to `text`.
       * @example "The aria-label of close dialog button"
       */
      note?: string;
      variables?: Record<GetVariables<Text>, ReactNode>;
      tags?: Record<GetTags<Text>, (children: ReactNode) => ReactNode>;
    },
  ): ReactNode;
}

const REGEX_VAR = /\\?\{([^}]+)\}/g;

export function useTranslations({ note }: HookOptions = {}): TranslationsHook {
  const translations = use(Context);
  return useMemo(() => fromTranslations(translations, { note }), [translations, note]);
}

/** create a translation function from a translations object (e.g. outside React) */
export function fromTranslations(
  translations: Partial<Record<string, string>>,
  { note: hookNote }: HookOptions = {},
): TranslationsHook {
  const res: TranslationsHook = (rawText, opts = {}) => {
    const { note, variables } = opts;
    const notes: string[] = [];
    if (hookNote) notes.push(hookNote);
    if (note) notes.push(note);
    const k = encodeKey(rawText, notes);
    let text = translations[k] ?? rawText;

    if (variables) {
      text = text.replaceAll(REGEX_VAR, (m, name: string) => {
        if (m[0] === "\\") return m.slice(1);
        if (name in variables) return variables[name as never];
        return m;
      });
    }

    return text;
  };

  res.jsx = (rawText, opts = {}) => {
    const { note, variables, tags } = opts;
    const notes: string[] = [];
    if (hookNote) notes.push(hookNote);
    if (note) notes.push(note);
    const k = encodeKey(rawText, notes);
    const text = translations[k] ?? rawText;
    if (tags) {
      return onJsx(text, variables, tags);
    }
    if (variables) {
      return onJsxVariables(text, variables);
    }
    return text;
  };

  res.translations = translations;
  return res;
}

function encodeKey(text: string, notes: string[]): string {
  return text + notes.map((n) => `(${n})`).join("");
}

function onJsxVariables(text: string, variables: Record<string, ReactNode>): ReactNode {
  let idx = 0;
  const out: ReactNode[] = [];
  for (const match of text.matchAll(REGEX_VAR)) {
    const [s, name] = match as unknown as [string, string];
    if (idx < match.index) {
      out.push(text.slice(idx, match.index));
    }
    idx = match.index + s.length;

    if (s[0] === "\\") {
      out.push(s.slice(1));
    } else if (name in variables) {
      out.push(enforceElementKey(variables[name], idx));
    } else {
      out.push(s);
    }
  }
  out.push(text.slice(idx));
  return out;
}

const REGEX_TAG = /\\?<([^>]+)>/g;

function onJsx(
  text: string,
  variables: Record<string, ReactNode> | undefined,
  tags: Record<string, (children: ReactNode) => ReactNode>,
) {
  const stack: { children: ReactNode[]; tag?: string }[] = [{ children: [] }];
  let idx = 0;
  const closeTag = () => {
    const current = stack.pop()!;

    stack[stack.length - 1]!.children.push(
      enforceElementKey(
        current.tag && tags[current.tag] ? tags[current.tag]!(current.children) : current.children,
        idx,
      ),
    );
  };

  for (const match of text.matchAll(REGEX_TAG)) {
    const current = stack[stack.length - 1]!;
    const [s, content] = match as unknown as [string, string];

    if (idx < match.index) {
      const str = text.slice(idx, match.index);
      current.children.push(variables ? onJsxVariables(str, variables) : str);
    }
    idx = match.index + s.length;

    if (s[0] === "\\") {
      const str = s.slice(1);
      current.children.push(variables ? onJsxVariables(str, variables) : str);
    } else if (content[0] === "/") {
      const name = content.slice(1);
      // ignore close tags without open tags
      if (current.tag !== name) {
        current.children.push(variables ? onJsxVariables(s, variables) : s);
      } else {
        closeTag();
      }
    } else if (content.at(-1) === "/") {
      const name = content.slice(0, -1);

      if (tags[name]) {
        current.children.push(enforceElementKey(tags[name](undefined), idx));
      }
    } else {
      stack.push({ children: [], tag: content });
    }
  }

  if (idx < text.length) {
    const str = text.slice(idx);
    stack[stack.length - 1]!.children.push(variables ? onJsxVariables(str, variables) : str);
  }

  while (stack.length > 1) closeTag();
  return stack[0]!.children;
}

function enforceElementKey(value: ReactNode, key: number | string): ReactNode {
  if (isValidElement(value)) return cloneElement(value, { key });
  return value;
}
