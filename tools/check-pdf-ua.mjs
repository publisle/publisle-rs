/** Independently validate heading destinations after pagination in both PDF versions. */
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve, join } from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { validatePdf } from "./validate-pdf-ua.mjs";
const [compiler, validator, output, fontDirectory] = process.argv.slice(2);
assert.equal(
  process.argv.length,
  6,
  "Usage: node tools/check-pdf-ua.mjs COMPILER VALIDATOR OUTPUT FONT_DIRECTORY",
);
await mkdir(output, { recursive: true });
const text = (value) => [{ type: "text", value }];
for (const pdfStandard of ["ua-1", "ua-2"]) {
  const blocks = [
    { kind: "heading", level: 1, inlines: text("First heading") },
    {
      kind: "heading",
      level: 2,
      inlines: [
        {
          type: "link",
          url: "https://example.com",
          value: "Linked heading",
        },
      ],
    },
    ...Array.from({ length: 90 }, () => ({
      kind: "paragraph",
      inlines: text(
        "A paragraph to force pagination and check final heading destinations.",
      ),
    })),
    { kind: "heading", level: 1, inlines: text("Later heading") },
  ];
  const input = {
    schemaVersion: 1,
    template: "article",
    templateVersion: "1",
    pdfStandard,
    article: {
      title: "Bookmark accessibility regression",
      authors: ["Ada"],
      language: "en",
      blocks,
    },
    citations: [],
    bibliography: [],
    assets: [],
    fonts: [
      { path: resolve(fontDirectory, "DejaVuSerif.ttf"), role: "regular" },
    ],
    templateData: {},
  };
  const path = join(output, `${pdfStandard}.json`);
  const pdf = join(output, `${pdfStandard}.pdf`);
  await writeFile(path, JSON.stringify(input));
  const { stdout } = await promisify(execFile)(resolve(compiler), [path, pdf], {
    timeout: 30000,
  });
  assert.equal(JSON.parse(stdout).ok, true);
  await validatePdf(pdf, {
    validator: resolve(validator),
    flavour: pdfStandard.replace("-", ""),
    report: join(output, `${pdfStandard}.xml`),
  });
  console.log(`${pdfStandard}: paginated heading bookmarks passed veraPDF`);
}
