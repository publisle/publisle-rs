import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile, readdir } from "node:fs/promises";
const root = new URL("../conformance/", import.meta.url);
const lock = JSON.parse(await readFile(new URL("lock.json", root), "utf8"));
assert.equal(lock.schemaVersion, 1);
assert.match(lock.sourceCommit, /^[a-f0-9]{40}$/u);
for (const version of Object.values(lock.packages))
  assert.match(version, /^\d+\.\d+\.\d+$/u);
if (process.argv.includes("--upstream")) {
  for (const [name, version] of Object.entries(lock.packages)) {
    const response = await fetch(
      `https://raw.githubusercontent.com/publisle/publisle/${lock.sourceCommit}/packages/${name}/package.json`,
    );
    assert(response.ok, `${name}: upstream HTTP ${response.status}`);
    assert.equal(
      (await response.json()).version,
      version,
      `${name}: version mismatch`,
    );
  }
}
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
await walk("regressions");
assert.deepEqual(
  paths.sort(),
  Object.keys({ ...lock.files, ...lock.localFiles }).sort(),
  "Locked file set changed",
);
for (const [path, expected] of Object.entries({
  ...lock.files,
  ...lock.localFiles,
})) {
  assert.match(
    path,
    /^(schemas|fixtures|figures|regressions)\/[a-zA-Z0-9_.-]+\.(json|svg)$/u,
  );
  const bytes = await readFile(new URL(path, root));
  assert.equal(
    createHash("sha256").update(bytes).digest("hex"),
    expected,
    path,
  );
  if (path.endsWith(".json")) JSON.parse(bytes);
  if (
    process.argv.includes("--upstream") &&
    !Object.hasOwn(lock.localFiles ?? {}, path)
  ) {
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

assert.deepEqual(
  await readFile(
    new URL("../crates/publisle-math/src/regression-math.json", root),
  ),
  await readFile(new URL("regressions/math.json", root)),
  "Published math regression fixture differs from conformance",
);

assert.deepEqual(
  await readFile(
    new URL("../crates/publisle-pdf/src/regression-math.json", root),
  ),
  await readFile(new URL("regressions/math.json", root)),
  "Published PDF regression fixture differs from conformance",
);
