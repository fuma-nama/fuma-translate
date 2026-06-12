import { T } from "@fuma-translate/react";

export function RichText() {
  return (
    <>
      <T
        text="Click <a>here</a> to continue"
        tags={{
          a: () => <a />,
        }}
      />
      <T
        text="Or <signup/> today"
        note="landing page"
        tags={{
          signup: () => <button />,
        }}
      />
      <T text="Hello {user}" variables={{ user: <strong>Ada</strong> }} />
    </>
  );
}
