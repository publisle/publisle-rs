import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { setTimeout } from "node:timers/promises";
import { readdir, readFile } from "node:fs/promises";
import { realpathSync, existsSync } from "node:fs";
import { delimiter, join } from "node:path";
function npmCli() {
  const path = (process.env.PATH ?? "")
    .split(delimiter)
    .map((p) => join(p, "npm"))
    .find(existsSync);
  assert(path, "npm is required");
  return realpathSync(path);
}
async function releasePackages(root) {
  const paths = await readdir(join(root, "npm"));
  return Promise.all(
    paths.map(async (name) => ({
      manifest: JSON.parse(
        await readFile(join(root, "npm", name, "package.json"), "utf8"),
      ),
    })),
  );
}

const args = process.argv.slice(2);
assert(
  args.length === 0 ||
    (args.length === 1 && ["--apply", "--audit"].includes(args[0] ?? "")),
  "Use node tools/npm-trusted-publishers.mjs [--apply|--audit]",
);
const packages = await releasePackages(resolve(import.meta.dirname, ".."));
const cli = process.env["NPM_CLI_PATH"] ?? npmCli();
if (args[0] === "--apply" || args[0] === "--audit") {
  const version = execFileSync(process.execPath, [cli, "--version"], {
    cwd: tmpdir(),
    encoding: "utf8",
  }).trim();
  const [major = 0, minor = 0] = version.split(".").map(Number);
  assert(
    major > 12 || (major === 12 && minor >= 2),
    "Install npm >=12.2 before applying.",
  );
}
for (const [index, { manifest }] of packages.entries()) {
  const command = [
    "trust",
    "github",
    manifest.name,
    "--repo",
    "publisle/publisle-rs",
    "--file",
    "release.yml",
    "--allow-publish",
    "--yes",
    "--registry",
    "https://registry.npmjs.org/",
  ];
  console.log(`npm ${command.join(" ")}`);
  if (args[0] === "--apply" || args[0] === "--audit") {
    const output = execFileSync(
      process.execPath,
      [
        cli,
        "trust",
        "list",
        manifest.name,
        "--json",
        "--progress=false",
        "--registry",
        "https://registry.npmjs.org/",
      ],
      {
        cwd: tmpdir(),
        encoding: "utf8",
        stdio: ["inherit", "pipe", "inherit"],
      },
    ).trim();
    const configurations = output
      ? output.split(/\n(?=\{)/u).map((value) => JSON.parse(value))
      : [];
    const matching = configurations.some(
      (config) =>
        config.type === "github" &&
        config.repository === "publisle/publisle-rs" &&
        config.file === "release.yml" &&
        !config.environment &&
        config.permissions?.includes("createPackage"),
    );
    if (matching) console.log(`${manifest.name}: already configured`);
    else if (args[0] === "--audit")
      throw new Error(
        `${manifest.name}: release.yml trusted publisher is missing`,
      );
    else
      execFileSync(process.execPath, [cli, ...command, "--progress=false"], {
        cwd: tmpdir(),
        stdio: "inherit",
      });
    if (index < packages.length - 1) await setTimeout(2000);
  }
}
console.log(
  `${String(packages.length)} public packages ${args[0] === "--audit" ? "audited" : args[0] === "--apply" ? "configured" : "previewed; use --apply to configure"}.`,
);
