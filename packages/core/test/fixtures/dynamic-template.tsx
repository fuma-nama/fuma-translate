import { useTranslations } from "@fuma-translate/react";

export function DynamicTemplate() {
  const t = useTranslations();
  const name = "world";

  return t(`Hello ${name}`);
}
