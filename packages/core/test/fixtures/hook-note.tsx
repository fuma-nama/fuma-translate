import { useTranslations } from "@fuma-translate/react";

declare const condition: boolean;

export function WithHookNote() {
  const t = useTranslations({ note: "settings page" });

  return (
    <>
      {t("Save")}
      {t("Cancel", { note: "dialog button" })}
    </>
  );
}

export function ConditionalHookNote() {
  const t = useTranslations(condition ? { note: "light mode" } : { note: "dark mode" });

  return t("Theme");
}
