import { useTranslations } from "@fuma-translate/react";

export function Renamed() {
  const myT = useTranslations();

  return (
    <>
      {myT("Hello from myT")}
      {myT.jsx("Read <link>docs</link>", {
        tags: {
          link: (children) => <a>{children}</a>,
        },
      })}
    </>
  );
}
