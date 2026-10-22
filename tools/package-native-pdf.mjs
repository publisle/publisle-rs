/** Package only CI-built binaries and the exact reviewed font bundle; no installers. */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile, writeFile, mkdir, copyFile, chmod } from "node:fs/promises";
import { resolve, join, basename } from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
const root = resolve(import.meta.dirname, "..");
const [
  binary,
  manifestPath,
  licensesDirectory,
  platform,
  architecture,
  destination,
] = process.argv.slice(2);
assert.equal(
  process.argv.length,
  8,
  "Usage: node tools/package-native-pdf.mjs BINARY FONTS.json LICENSES_DIRECTORY PLATFORM ARCH NEW_DIRECTORY",
);
assert(
  [
    "linux-x64",
    "linux-arm64",
    "darwin-x64",
    "darwin-arm64",
    "win32-x64",
  ].includes(`${platform}-${architecture}`),
  "Unsupported platform",
);
const { version } = JSON.parse(
  await readFile(join(root, "native/pdf/package-version.json"), "utf8"),
);
const { stdout } = await promisify(execFile)(resolve(binary), ["--version"], {
  timeout: 10000,
  maxBuffer: 65536,
});
assert.equal(
  stdout.trim(),
  `publisle-pdf ${version}`,
  "Wrong compiler version",
);
const directory = resolve(destination);
await mkdir(directory, { recursive: false });
await mkdir(join(directory, "bin"));
await mkdir(join(directory, "fonts"));
await mkdir(join(directory, "licenses"));
const executable = `bin/publisle-pdf${platform === "win32" ? ".exe" : ""}`;
await copyFile(binary, join(directory, executable));
await chmod(join(directory, executable), 0o755);
const files = {};
const record = async (file) => {
  files[file] = createHash("sha256")
    .update(await readFile(join(directory, file)))
    .digest("hex");
};
await record(executable);
const fonts = JSON.parse(await readFile(manifestPath, "utf8"));
assert(Array.isArray(fonts) && fonts.length > 0 && fonts.length <= 16);
const names = new Set();
for (const font of fonts) {
  const name = basename(font.path);
  assert(!names.has(name), "Duplicate font filename");
  names.add(name);
  const source = resolve(resolve(manifestPath, ".."), font.path);
  await copyFile(source, join(directory, "fonts", name));
  font.path = `fonts/${name}`;
  await record(font.path);
}
await writeFile(join(directory, "fonts.json"), JSON.stringify(fonts, null, 2));
await record("fonts.json");
for (const name of ["noto-core.txt", "noto-cjk.txt", "dejavu.txt"]) {
  await copyFile(
    join(licensesDirectory, name),
    join(directory, "licenses", name),
  );
  await record(`licenses/${name}`);
}
await copyFile(join(root, "LICENSE"), join(directory, "LICENSE"));
await record("LICENSE");
const artifact = {
  schemaVersion: 1,
  inputSchemaVersion: 1,
  compilerVersion: version,
  platform,
  architecture,
  files,
};
await writeFile(
  join(directory, "artifact.json"),
  JSON.stringify(artifact, null, 2),
);
const name = `@publisle/pdf-native-${platform}-${architecture}`;
const manifest = {
  name,
  version,
  description: `Pinned Publisle native PDF compiler and fonts for ${platform}/${architecture}`,
  type: "module",
  license: "MIT",
  os: [platform],
  cpu: [architecture],
  ...(platform === "linux" ? { libc: ["glibc"] } : {}),
  engines: { node: ">=24" },
  exports: {
    "./package.json": "./package.json",
    "./artifact.json": "./artifact.json",
    "./fonts.json": "./fonts.json",
  },
  files: [
    "bin",
    "fonts",
    "licenses",
    "fonts.json",
    "artifact.json",
    "LICENSE",
    "README.md",
  ],
  publishConfig: { access: "public" },
  repository: {
    type: "git",
    url: "git+https://github.com/VoidReg/publisle.git",
    directory: "native/pdf",
  },
};
await writeFile(
  join(directory, "package.json"),
  JSON.stringify(manifest, null, 2),
);
await writeFile(
  join(directory, "README.md"),
  `# ${name}\n\nPinned native compiler and font assets for Publisle Research ${version}. No Python, TeX, runtime downloads, or install scripts. Font licenses are in licenses/.\n`,
);
console.log(
  JSON.stringify({
    name,
    version,
    directory,
    artifactSha256: createHash("sha256")
      .update(await readFile(join(directory, "artifact.json")))
      .digest("hex"),
  }),
);
