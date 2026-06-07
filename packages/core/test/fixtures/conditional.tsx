import { useTranslations } from "@fuma-translate/react";

declare const condition: boolean;

export function Conditional() {
  const t = useTranslations();

  return t("Theme", condition ? { note: "light mode" } : { note: "dark mode" });
}
