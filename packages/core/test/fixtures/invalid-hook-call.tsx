import { useTranslations } from "@fuma-translate/react";

export function InvalidHookCall() {
  const t = useTranslations();

  // @ts-expect-error -- invalid usage
  t("Bad options", "not-an-object");
}
