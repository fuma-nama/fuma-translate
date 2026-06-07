"use client";
import { createContext, use, useMemo, type ReactNode } from "react";

type Translations = Record<string, string>;

const Context = createContext<Translations>({});

/** add translations, you can stack multiple <TranslationProvider /> to override/extend translations */
export function TranslationProvider({
  translations,
  children,
}: {
  translations: Translations;
  children: ReactNode;
}) {
  const parent = use(Context);
  return (
    <Context value={useMemo(() => ({ ...parent, ...translations }), [parent, translations])}>
      {children}
    </Context>
  );
}

type GetVariables<T extends string> = T extends `${string}{${infer K}}${infer After}`
  ? K | GetVariables<After>
  : never;

export interface TranslationsHook {
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

export function useTranslations(hookOptions?: {
  /** provide additional context to all t() calls */
  note?: string;
}): TranslationsHook {
  const translations = use(Context);

  return (rawText, opts = {}) => {
    const { note, variables } = opts;
    const notes: string[] = [];
    if (hookOptions?.note) notes.push(hookOptions.note);
    if (note) notes.push(note);
    const k = encodeKey(rawText, notes);
    let text = translations[k] ?? rawText;

    if (variables) {
      for (const k in variables) text = text.replaceAll(`{${k}}`, variables[k as never]);
    }

    return text;
  };
}

function encodeKey(text: string, notes: string[]): string {
  return text + notes.map((n) => `(${n})`).join("");
}
