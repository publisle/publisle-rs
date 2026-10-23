import assert from "node:assert/strict";
import { readFile, writeFile, readdir } from "node:fs/promises";
const root = new URL("../", import.meta.url);
const cargo = await readFile(
  new URL("crates/publisle-pdf/Cargo.toml", root),
  "utf8",
);
const version = cargo.match(/^version = "([^"]+)"$/mu)?.[1];
assert.match(version ?? "", /^\d+\.\d+\.\d+(?:-[a-zA-Z0-9.-]+)?$/u);
for (const name of await readdir(new URL("npm/", root))) {
  const path = new URL(`npm/${name}/package.json`, root);
  const manifest = JSON.parse(await readFile(path, "utf8"));
  if (process.argv.includes("--check"))
    assert.equal(manifest.version, version, name);
  else {
    manifest.version = version;
    await writeFile(path, JSON.stringify(manifest, null, 2) + "\n");
  }
}
