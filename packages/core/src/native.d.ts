declare module "../native/native.js" {
  export interface CompileOutput {
    translationKeys: string[];
  }

  export function compileSync(input: string[], strict?: boolean): CompileOutput;
}
