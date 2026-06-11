"use client";
import { createContext, use, useMemo, type ReactNode } from "react";

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
        // keep as-is if unspecified
        return m;
      });
    }

    return text;
  };

  res.translations = translations;
  return res;
}

function encodeKey(text: string, notes: string[]): string {
  return text + notes.map((n) => `(${n})`).join("");
}
