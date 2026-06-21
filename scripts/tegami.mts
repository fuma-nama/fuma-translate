#!/usr/bin/env node
import { join } from "node:path";
import { x } from "tinyexec";
import { tegami } from "tegami";
import type { TegamiPlugin } from "tegami";
import { createCli } from "tegami/cli";
import { github } from "tegami/plugins/github";

async function run(cmd: string, args: string[], cwd: string) {
  const result = await x(cmd, args, { nodeOptions: { cwd } });
  if (result.exitCode !== 0) {
    throw new Error(`Failed to run ${cmd} ${args.join(" ")}`);
  }
}

function parseTargets() {
  return process.env.NAPI_TARGETS?.split(/\s+/).filter(Boolean) ?? [];
}

async function buildNativeTarget(coreDir: string, target: string) {
  const args = [
    "exec",
    "napi",
    "build",
    "--platform",
    "--release",
    "--esm",
    "--js",
    "native.js",
    "--dts",
    "native.d.ts",
    "--target",
    target,
    "--output-dir",
    "native",
  ];
  if (target.includes("musl")) args.push("-x");
  await run("pnpm", args, coreDir);
}

function buildOnPublish(): TegamiPlugin {
  return {
    name: "build-on-publish",
    async willPublish({ pkg }) {
      const targets = parseTargets();
      const publishMain = process.env.TEGAMI_PUBLISH_MAIN === "true";

      if (pkg.name === "fuma-translate") {
        const coreDir = join(this.cwd, "packages/core");

        if (targets.length > 0) {
          for (const target of targets) {
            await buildNativeTarget(coreDir, target);
          }
          await run("pnpm", ["run", "artifacts"], coreDir);
          await run(
            "pnpm",
            ["exec", "napi", "prepublish", "-t", "npm"],
            coreDir,
          );
        }

        if (!publishMain) return false as const;

        await run("pnpm", ["run", "build:js"], coreDir);
        return;
      }

      if (targets.length > 0 && !publishMain) return false as const;

      await run(
        "pnpm",
        ["exec", "turbo", "run", "build", "--filter", pkg.name],
        this.cwd,
      );
    },
  };
}

const paper = tegami({
  npm: {
    updateLockFile: true,
  },
  plugins: [
    buildOnPublish(),
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
