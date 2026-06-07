import { useTranslations } from "fuma-translate/react";

export function Basic() {
  const t = useTranslations();

  return (
    <>
      {t("Hello")}
      {t("Close", { note: "dialog button" })}
      {t(`Static template`)}
      {t("Hello {user}", { variables: { user: "Fuma" } })}
    </>
  );
}
