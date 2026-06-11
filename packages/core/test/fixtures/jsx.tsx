import { useTranslations } from "@fuma-translate/react";

export function RichText() {
  const t = useTranslations();

  return (
    <>
      {t.jsx("Click <a>here</a> to continue", {
        tags: {
          a: () => <a />,
        },
      })}
      {t.jsx("Or <signup/> today", {
        note: "landing page",
        tags: {
          signup: () => <button />,
        },
      })}
    </>
  );
}
