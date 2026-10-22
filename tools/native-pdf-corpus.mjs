/** Freeze legacy output from a reviewed source checkout, then compare native PDFs. */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile, writeFile, mkdir, mkdtemp, rm } from "node:fs/promises";
import { resolve, join, extname, relative } from "node:path";
import { tmpdir } from "node:os";
import { pathToFileURL } from "node:url";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { compileJob } from "../deploy/vps/server.mjs";
import { TEMPLATES, validateRequest } from "../deploy/vps/limits.mjs";
import { comparePdf } from "./native-pdf-parity.mjs";
const execute = promisify(execFile);
const root = resolve(import.meta.dirname, "..");
const referenceRoot = resolve(process.env.PUBLISLE_REFERENCE_SOURCE ?? "");
const output = resolve(
  process.env.PDF_TEST_ARTIFACTS ?? ".local/native-parity",
);
const referenceImage = process.env.PUBLISLE_REFERENCE_WORKER_IMAGE;
const candidateImage = process.env.PDF_TEST_NATIVE_WORKER_IMAGE;
assert(
  referenceRoot !== root && referenceImage && candidateImage,
  "An independent frozen reference checkout and both immutable worker image IDs are required",
);
assert(
  /^sha256:[a-f0-9]{64}$/u.test(referenceImage) &&
    /^sha256:[a-f0-9]{64}$/u.test(candidateImage),
  "Use immutable worker image IDs",
);
const reference = JSON.parse(
  await readFile(join(root, "native/pdf/reference.json"), "utf8"),
);
assert.equal(
  (
    await execute("git", ["-C", referenceRoot, "rev-parse", "HEAD"])
  ).stdout.trim(),
  reference.sourceCommit,
  "Wrong reference checkout",
);
for (const [file, expected] of Object.entries(reference.files)) {
  assert(!file.includes("..") && !file.startsWith("/"));
  assert.equal(
    createHash("sha256")
      .update(await readFile(join(referenceRoot, file)))
      .digest("hex"),
    expected,
    `Changed reference source: ${file}`,
  );
}
const { playgroundExamples } = await import(
  pathToFileURL(join(referenceRoot, "sites/www/src/lib/playground-examples.ts"))
    .href
);
const fixtures = [];
for (const template of TEMPLATES) {
  const file =
    template.id === "article-arabic" ? "article-arabic.md" : "ieee-journal.md";
  const markdown = (
    await readFile(join(referenceRoot, "examples/articles", file), "utf8")
  ).replaceAll(
    'src="ieee-square-wave.pdf"',
    'src="https://fixture.invalid/paper-convergence.png"',
  );
  fixtures.push({
    id: template.id,
    template: template.id,
    markdown,
    engine: template.engines[0],
  });
}
for (const example of playgroundExamples)
  fixtures.push({
    id: `article-${example.id}`,
    template: "article",
    markdown: example.markdown,
    engine: "lualatex",
  });
await mkdir(output, { recursive: true });
const jobs = await mkdtemp(join(tmpdir(), "publisle-parity-jobs-"));
const report = {
  reference,
  referenceImage,
  candidateImage,
  fixtures: [],
  passed: true,
};
const images = {
  resolve: async () => [{ address: "93.184.216.34", family: 4 }],
  download: async ({ url }) => {
    const name = decodeURIComponent(url.pathname).replace(/^\/+/, "");
    assert(!name.includes("..") && !name.includes("\\"), "Unsafe fixture path");
    const file = join(
      referenceRoot,
      name === "paper-convergence.png"
        ? "examples/articles"
        : "sites/www/public",
      name,
    );
    assert(
      reference.files[relative(referenceRoot, file)],
      `Unpinned fixture asset ${file}`,
    );
    return {
      bytes: await readFile(file),
      mime: {
        ".svg": "image/svg+xml",
        ".png": "image/png",
        ".jpg": "image/jpeg",
      }[extname(file)],
    };
  },
};
try {
  for (const fixture of fixtures) {
    const directory = join(output, fixture.id);
    await mkdir(directory, { recursive: true });
    await writeFile(join(directory, "input.md"), fixture.markdown, {
      flag: "wx",
    });
    let entry = { id: fixture.id, template: fixture.template, passed: false };
    try {
      for (const [name, engine, image] of [
        ["reference", fixture.engine, referenceImage],
        ["candidate", "rust", candidateImage],
      ]) {
        const result = await compileJob(
          validateRequest(
            {
              markdown: fixture.markdown,
              template: fixture.template,
              engine,
              data: { country: "Jordan" },
            },
            TEMPLATES.map((t) => t.id),
          ),
          new AbortController().signal,
          {
            image,
            nativeImage: candidateImage,
            nativeTemplates: TEMPLATES.map((t) => t.id),
            imageDependencies: images,
            jobDirectory: jobs,
            assetBase: "https://fixture.invalid/",
          },
        );
        await writeFile(join(directory, `${name}.pdf`), result.pdf, {
          flag: "wx",
        });
      }
      const comparison = await comparePdf(
        join(directory, "reference.pdf"),
        join(directory, "candidate.pdf"),
        directory,
      );
      entry = {
        ...entry,
        passed: comparison.passed,
        differences: comparison.differences.length,
        scores: comparison.scores,
      };
    } catch (error) {
      entry.error = error.stack ?? String(error);
      await writeFile(join(directory, "failure.txt"), entry.error);
    }
    report.fixtures.push(entry);
    report.passed &&= entry.passed;
    await writeFile(
      join(output, "parity.json"),
      JSON.stringify(report, null, 2),
    );
    console.log(JSON.stringify(entry));
  }
} finally {
  await rm(jobs, { recursive: true, force: true });
}
if (!report.passed) process.exitCode = 1;
