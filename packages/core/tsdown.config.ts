import { defineConfig } from "tsdown";

export default defineConfig({
  entry: ["src/index.ts", "src/cli.ts"],
  target: "es2023",
  dts: true,
  exports: true,
  deps: {
    neverBundle: ["../native/native.js"],
  },
});
