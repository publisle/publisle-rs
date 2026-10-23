/** Publish only the five tarballs built and smoke-tested by this workflow run. */
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { publishPackages } from "./publication.mjs";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { readFile, readdir, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
const run = (cmd, args) =>
  execFileSync(cmd, args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
const version = (
  await readFile(
    new URL("../crates/publisle-pdf/Cargo.toml", import.meta.url),
    "utf8",
  )
).match(/^version = "([^"]+)"$/mu)?.[1];
assert.match(version ?? "", /^\d+\.\d+\.\d+$/u);
const commit = run("git", ["rev-parse", "HEAD"]);
const crateTag = `publisle-pdf-v${version}`;
// No release commit yet: allow release-plz to open its version PR first.
const tags = run("git", ["tag", "--points-at", commit]).split("\n");
if (!tags.includes(crateTag)) {
  console.log(
    `No ${crateTag} at ${commit}; npm publication waits for the crate release.`,
  );
  process.exit(0);
}
const crate = await fetch(
  `https://crates.io/api/v1/crates/publisle-pdf/${version}`,
  {
    headers: {
      "user-agent":
        "publisle-rs-release (https://github.com/publisle/publisle-rs)",
    },
  },
);
assert(crate.ok, `Crate ${version} must be published before npm`);
const stage = resolve(process.argv[2]);
const targets = [
  "darwin-arm64",
  "darwin-x64",
  "linux-arm64",
  "linux-x64",
  "win32-x64",
];
const files = (await readdir(stage)).filter((f) => f.endsWith(".tgz")).sort();
assert.equal(files.length, 5, "Expected all five validated platform artifacts");
const plan = [];
for (const file of files) {
  const path = join(stage, file);
  const manifest = JSON.parse(
    run("tar", ["-xOf", path, "package/package.json"]),
  );
  const target = manifest.name.replace("@publisle/pdf-native-", "");
  assert(targets.includes(target), "Unexpected package");
  assert(!plan.some((p) => p.name === manifest.name), "Duplicate platform");
  assert.equal(manifest.version, version);
  assert.equal(
    manifest.repository.url,
    "git+https://github.com/publisle/publisle-rs.git",
  );
  const artifact = JSON.parse(
    run("tar", ["-xOf", path, "package/artifact.json"]),
  );
  assert.equal(artifact.compilerVersion, version);
  assert.equal(`${artifact.platform}-${artifact.architecture}`, target);
  const bytes = await readFile(path);
  const integrity = `sha512-${createHash("sha512").update(bytes).digest("base64")}`;
  plan.push({ name: manifest.name, version, file, integrity });
}
async function registry(pkg) {
  const response = await fetch(
    `https://registry.npmjs.org/${encodeURIComponent(pkg.name)}/${version}?verify=${Date.now()}`,
    {
      signal: AbortSignal.timeout(30000),
      headers: { "cache-control": "no-cache" },
    },
  );
  if (response.status === 404) return null;
  assert(response.ok, `Registry HTTP ${response.status}`);
  return response.json();
}
const progress = join(stage, "accepted-uploads.json");
const saved = existsSync(progress)
  ? JSON.parse(await readFile(progress, "utf8"))
  : { commit, version, accepted: [] };
assert.equal(
  saved.commit,
  commit,
  "Upload progress belongs to another source commit",
);
assert.equal(
  saved.version,
  version,
  "Upload progress belongs to another version",
);
const accepted = new Set(saved.accepted);
await publishPackages(plan, {
  accepted,
  lookup: registry,
  wait: () => new Promise((ok) => setTimeout(ok, 5000)),
  save: async () =>
    writeFile(
      progress,
      JSON.stringify({ commit, version, accepted: [...accepted] }, null, 2) +
        "\n",
    ),
  upload: async (pkg) => {
    try {
      run("npm", [
        "publish",
        join(stage, pkg.file),
        "--access",
        "public",
        "--provenance",
        "--ignore-scripts",
        "--registry",
        "https://registry.npmjs.org/",
      ]);
    } catch (error) {
      throw new Error(String(error) + "\n" + String(error.stderr ?? ""));
    }
  },
});
// Verify the registry's actual downloaded bytes, not only its integrity metadata.
for (const pkg of plan) {
  const metadata = await registry(pkg);
  const response = await fetch(metadata.dist.tarball, {
    signal: AbortSignal.timeout(60000),
  });
  assert(response.ok, `Tarball HTTP ${response.status}`);
  const integrity = `sha512-${createHash("sha512")
    .update(Buffer.from(await response.arrayBuffer()))
    .digest("base64")}`;
  assert.equal(integrity, pkg.integrity, `Changed public tarball: ${pkg.name}`);
}
const evidence = join(stage, "native-release.json");
await writeFile(
  evidence,
  JSON.stringify(
    { schemaVersion: 1, commit, crateTag, version, packages: plan },
    null,
    2,
  ) + "\n",
);
run("gh", [
  "release",
  "upload",
  crateTag,
  ...files.map((f) => join(stage, f)),
  evidence,
  "--clobber",
]);
console.log(`Verified ${plan.length} public native packages at ${version}.`);
