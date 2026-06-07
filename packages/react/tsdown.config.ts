import { defineConfig } from "tsdown";

export default defineConfig({
  entry: ["src/index.tsx"],
  target: "es2023",
  dts: true,
  exports: true,
  deps: {
    onlyBundle: [],
  },
});
