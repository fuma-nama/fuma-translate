import { fromTranslations } from "@fuma-translate/react";

const translations = {
  Hello: "Hi",
};

export function Server() {
  const t = fromTranslations(translations);

  return t("Server Hello");
}

export function WithNote() {
  const t = fromTranslations(translations, { note: "admin panel" });

  return t("Dashboard");
}
