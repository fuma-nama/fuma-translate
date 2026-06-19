#!/usr/bin/env node
import { tegami } from "tegami";
import { createCli } from "tegami/cli";
import { github } from "tegami/plugins/github";

const paper = tegami({
  npm: {
    updateLockFile: true,
  },
  plugins: [
    github({
      repo: "fuma-nama/fuma-translate",
      cli: {
        versionPr: {
          base: "dev",
        },
      },
    }),
  ],
});

await createCli(paper).parseAsync();
