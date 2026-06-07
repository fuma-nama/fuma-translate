import { useTranslations } from "fuma-translate/react";

export function DynamicKey() {
  const t = useTranslations();
  const key = "Hello";

  return t(key);
}
