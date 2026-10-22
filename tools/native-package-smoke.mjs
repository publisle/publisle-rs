/** Compile using the packaged binary/fonts with an empty PATH: no Python or TeX. */
import assert from "node:assert/strict";
import { readFile, writeFile, mkdtemp, rm, mkdir } from "node:fs/promises";
import { resolve, join } from "node:path";
import { tmpdir } from "node:os";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { createHash } from "node:crypto";
const directory = resolve(process.argv[2] ?? "");
assert.equal(
  process.argv.length,
  3,
  "Usage: node tools/native-package-smoke.mjs PACKAGE_DIRECTORY",
);
const artifact = JSON.parse(
  await readFile(join(directory, "artifact.json"), "utf8"),
);
assert.equal(artifact.platform, process.platform);
assert.equal(artifact.architecture, process.arch);
for (const [file, sha256] of Object.entries(artifact.files))
  assert.equal(
    createHash("sha256")
      .update(await readFile(join(directory, file)))
      .digest("hex"),
    sha256,
    file,
  );
const fonts = JSON.parse(
  await readFile(join(directory, "fonts.json"), "utf8"),
).map((font) => ({ ...font, path: resolve(directory, font.path) }));
const executable = join(
  directory,
  "bin",
  `publisle-pdf${process.platform === "win32" ? ".exe" : ""}`,
);
const stage = await mkdtemp(join(tmpdir(), "publisle-offline-"));
try {
  const emptyPath = join(stage, "empty-path");
  await mkdir(emptyPath);
  for (const pdfStandard of ["ua-1", "ua-2"]) {
    const input = {
      schemaVersion: 1,
      template: "article",
      templateVersion: "0.1.0-experimental",
      pdfStandard,
      article: {
        title: "Offline native compiler",
        authors: ["Ada"],
        language: "en",
        blocks: [
          {
            kind: "paragraph",
            inlines: [{ type: "text", value: "No Python or TeX required." }],
          },
          {
            kind: "math",
            value: "\\sqrt[3]{x^2+1}",
            display: true,
            label: null,
          },
        ],
      },
      citations: [],
      bibliography: [],
      assets: [],
      fonts,
      templateData: {},
    };
    const path = join(stage, `${pdfStandard}.json`);
    const output = join(stage, `${pdfStandard}.pdf`);
    await writeFile(path, JSON.stringify(input));
    const { stdout } = await promisify(execFile)(executable, [path, output], {
      timeout: 30000,
      maxBuffer: 65536,
      env: { ...process.env, PATH: emptyPath },
    });
    const report = JSON.parse(stdout);
    assert.equal(report.ok, true);
    assert(report.pages > 0);
    assert.equal(report.compilerVersion, artifact.compilerVersion);
    assert.equal(
      (await readFile(output)).subarray(0, 8).toString(),
      pdfStandard === "ua-1" ? "%PDF-1.7" : "%PDF-2.0",
    );
  }
  console.log(
    `${process.platform}/${process.arch}: packaged compiler runs offline with an empty PATH`,
  );
} finally {
  await rm(stage, { recursive: true, force: true });
}
