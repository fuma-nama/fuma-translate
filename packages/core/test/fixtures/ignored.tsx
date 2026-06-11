import { useTranslations } from "@fuma-translate/react";
// @ts-expect-error -- faked
import { t } from "<unknown>";

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

export function WithoutHook() {
  // invalid call, not from a translation hook — ignored by default (strict)
  t("test", "test2");

  // valid call, not from a translation hook — ignored by default (strict)
  t("Without Hook");
}

export function ShadowedHook() {
  const t = useTranslations();

  function inner() {
    // invalid call, shadowed local t — ignored
    t("Shadowed invalid", "not-options");

    function t(_v: string, _r?: string) {}
  }

  inner();

  t("From hook");
}
