export function encodeKey(text: string, note?: string): string {
  return note ? `${text}(${note})` : text;
}
