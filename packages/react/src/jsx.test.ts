import { createElement as h } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { fromTranslations } from "./index.js";

function render(node: React.ReactNode): string {
  return renderToStaticMarkup(h("div", null, node));
}

describe("t.jsx", () => {
  const t = fromTranslations({});

  it("wraps paired tags", () => {
    expect(
      render(
        t.jsx("Click <a>here</a>", {
          tags: {
            a: (children) => h("a", { href: "/login" }, children),
          },
        }),
      ),
    ).toBe('<div>Click <a href="/login">here</a></div>');
  });

  it("renders self-closing tags", () => {
    expect(
      render(
        t.jsx("Go to <signup/> now", {
          tags: {
            signup: () => h("button", { type: "button" }, "Sign up"),
          },
        }),
      ),
    ).toBe('<div>Go to <button type="button">Sign up</button> now</div>');
  });

  it("interpolates variables as React nodes", () => {
    expect(
      render(
        t.jsx("Hello {name}", {
          variables: { name: h("strong", null, "Ada") },
        }),
      ),
    ).toBe("<div>Hello <strong>Ada</strong></div>");
  });

  it("supports nested tags", () => {
    expect(
      render(
        t.jsx("Read <bold><link>docs</link></bold>", {
          tags: {
            bold: h("b", null),
            link: (children) => h("a", { href: "/docs" }, children),
          },
        }),
      ),
    ).toBe('<div>Read <b><a href="/docs">docs</a></b></div>');
  });

  it("preserves backslash-escaped markup", () => {
    expect(
      render(
        t.jsx("Use \\{brackets} and \\<tags>", {
          tags: {},
          variables: {},
        }),
      ),
    ).toBe("<div>Use {brackets} and &lt;tags&gt;</div>");
  });

  it("unwraps unknown tags to their children", () => {
    expect(
      render(
        t.jsx("Click <a>here</a>", {
          tags: {} as never,
        }),
      ),
    ).toBe("<div>Click here</div>");
  });
});
