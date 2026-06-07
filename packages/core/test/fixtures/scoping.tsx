import { useTranslations } from "fuma-translate/react";

export function Outer() {
  const t = useTranslations();
  const fn = () => t("From outer scope");

  return fn();
}

export function Shadowed() {
  const t = useTranslations();
  t("Before block");

  {
    const t = useTranslations();
    t("Inside block");
  }

  return null;
}
