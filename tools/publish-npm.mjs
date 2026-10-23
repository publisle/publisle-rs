/** Publish only the five tarballs built and smoke-tested by this workflow run. */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { readFile, readdir, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
const run = (cmd, args) =>
  execFileSync(cmd, args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "inherit"],
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
  const response = await fetch(
    `https://registry.npmjs.org/${encodeURIComponent(manifest.name)}/${version}`,
  );
  assert(
    response.ok || response.status === 404,
    `Registry HTTP ${response.status}`,
  );
  if (response.ok)
    assert.equal(
      (await response.json()).dist.integrity,
      integrity,
      `Conflicting immutable package ${manifest.name}`,
    );
  plan.push({
    name: manifest.name,
    version,
    file,
    integrity,
    existing: response.ok,
  });
}
// Finish the whole immutable-version preflight before the first upload.
for (const pkg of plan) {
  if (!pkg.existing)
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
  let verified = false;
  for (let attempt = 0; attempt < 12; attempt++) {
    const response = await fetch(
      `https://registry.npmjs.org/${encodeURIComponent(pkg.name)}/${version}`,
    );
    if (response.ok) {
      assert.equal((await response.json()).dist.integrity, pkg.integrity);
      verified = true;
      break;
    }
    assert.equal(response.status, 404);
    await new Promise((ok) => setTimeout(ok, 5000));
  }
  assert(verified, `Registry did not confirm ${pkg.name}`);
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
