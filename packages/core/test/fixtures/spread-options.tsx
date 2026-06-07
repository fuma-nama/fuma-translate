import { useTranslations } from "@fuma-translate/react";

const opts = { note: "shared" };

export function SpreadOptions() {
  const t = useTranslations();

  return t("Hello", { ...opts });
}
