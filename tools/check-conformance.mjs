import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile, readdir } from "node:fs/promises";
const root = new URL("../conformance/", import.meta.url);
const lock = JSON.parse(await readFile(new URL("lock.json", root), "utf8"));
assert.equal(lock.schemaVersion, 1);
assert.match(lock.sourceCommit, /^[a-f0-9]{40}$/u);
for (const version of Object.values(lock.packages))
  assert.match(version, /^\d+\.\d+\.\d+$/u);
const paths = [];
async function walk(directory) {
  for (const entry of await readdir(new URL(directory + "/", root), {
    withFileTypes: true,
  })) {
    const path = `${directory}/${entry.name}`;
    if (entry.isDirectory()) await walk(path);
    else paths.push(path);
  }
}
await walk("schemas");
await walk("fixtures");
await walk("figures");
assert.deepEqual(
  paths.sort(),
  Object.keys(lock.files).sort(),
  "Locked file set changed",
);
for (const [path, expected] of Object.entries(lock.files)) {
  assert.match(
    path,
    /^(schemas|fixtures|figures)\/[a-zA-Z0-9_.-]+\.(json|svg)$/u,
  );
  const bytes = await readFile(new URL(path, root));
  assert.equal(
    createHash("sha256").update(bytes).digest("hex"),
    expected,
    path,
  );
  if (path.endsWith(".json")) JSON.parse(bytes);
  if (process.argv.includes("--upstream")) {
    const upstream = path.startsWith("figures/")
      ? `sites/www/public/${path.slice(8)}`
      : `packages/contracts/${path}`;
    const response = await fetch(
      `https://raw.githubusercontent.com/publisle/publisle/${lock.sourceCommit}/${upstream}`,
    );
    assert(response.ok, `${path}: upstream HTTP ${response.status}`);
    assert.equal(
      createHash("sha256")
        .update(Buffer.from(await response.arrayBuffer()))
        .digest("hex"),
      expected,
      `${path}: upstream mismatch`,
    );
  }
}
console.log(`Verified ${paths.length} pinned conformance files.`);
