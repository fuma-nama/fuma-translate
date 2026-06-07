export function encodeKey(text: string, notes: string[]): string {
  return text + notes.map((n) => `(${n})`).join("");
}
