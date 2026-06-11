import { useTranslations } from "@fuma-translate/react";

export function EscapedVariables() {
  const t = useTranslations();
  return (
    <>
      {t("Hello {user}", {
        variables: { user: "..." },
      })}
      {t("Show \\{literal} braces {var}", {
        variables: {
          var: "...",
        },
      })}
    </>
  );
}
