function T({ text }: { text: string }) {
  return <span>{text}</span>;
}

export function FakeT() {
  return <T text="Should not be extracted" />;
}
