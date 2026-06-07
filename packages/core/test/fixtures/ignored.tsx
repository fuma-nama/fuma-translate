import { useTranslations } from "fuma-translate/react";

function translate(label: string) {
  return label;
}

export function Ignored() {
  const t = useTranslations();

  return (
    <>
      {t("Tracked")}
      {translate("Ignored")}
    </>
  );
}
