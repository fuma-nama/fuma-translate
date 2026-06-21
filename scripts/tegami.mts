#!/usr/bin/env node
import { join } from "node:path";
import { x } from "tinyexec";
import { tegami } from "tegami";
import type { PublishResult, TegamiPlugin } from "tegami";
import { createCli } from "tegami/cli";
import { github } from "tegami/plugins/github";

async function run(cmd: string, args: string[], cwd: string) {
  const result = await x(cmd, args, { nodeOptions: { cwd } });
  if (result.exitCode !== 0) {
    throw new Error(`Failed to run ${cmd} ${args.join(" ")}`);
  }
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
      const bindingOnly = process.env.TEGAMI_BINDING_ONLY === "1";
      const target = process.env.NAPI_TARGET;

      if (bindingOnly && pkg.name !== "fuma-translate") return false as const;

      if (pkg.name === "fuma-translate") {
        const coreDir = join(this.cwd, "packages/core");
        if (bindingOnly) {
          if (!target) return false;
          await buildNativeTarget(coreDir, target);
          await run("pnpm", ["run", "artifacts"], coreDir);
          await run("pnpm", ["exec", "napi", "prepublish", "-t", "npm"], coreDir);
          return false;
        }
        await run("pnpm", ["run", "build:js"], coreDir);
        return;
      }

      await run("pnpm", ["exec", "turbo", "run", "build", "--filter", pkg.name], this.cwd);
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

function parseTargets() {
  return (
    process.env.NAPI_TARGETS?.split(/\s+/).filter(Boolean) ??
    (process.env.NAPI_TARGET ? [process.env.NAPI_TARGET] : [])
  );
}

await createCli(paper, {
  publish: async () => {
    let result: PublishResult = { state: "skipped" };
    const targets = parseTargets();

    for (const target of targets) {
      process.env.NAPI_TARGET = target;
      process.env.TEGAMI_BINDING_ONLY = "1";
      result = await paper.publish();
      if (result.state === "skipped") return result;
    }

    delete process.env.NAPI_TARGET;
    delete process.env.TEGAMI_BINDING_ONLY;

    if (process.env.TEGAMI_PUBLISH_MAIN === "true") {
      return paper.publish();
    }

    return result;
  },
}).parseAsync();
