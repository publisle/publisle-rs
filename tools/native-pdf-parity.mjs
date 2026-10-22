/** Compare frozen legacy PDFs and native candidates. Run on CI/VPS, not the desktop. */
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { createHash } from "node:crypto";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdir, mkdtemp, rm } from "node:fs/promises";
import { resolve, join } from "node:path";
import { tmpdir } from "node:os";
import { pathToFileURL } from "node:url";
import sax from "sax";
const execute = promisify(execFile);
const require = createRequire(
  new URL("../packages/pdf-service/package.json", import.meta.url),
);

export function textGeometry(xml) {
  const parser = sax.parser(true);
  const pages = [];
  let line;
  let word;
  parser.onerror = (error) => {
    throw error;
  };
  parser.onopentag = ({ name, attributes }) => {
    if (name === "page")
      pages.push({
        width: Number(attributes.width),
        height: Number(attributes.height),
        lines: [],
      });
    if (name === "line") {
      line = [];
      assert(pages.length);
      pages.at(-1).lines.push(line);
    }
    if (name === "word") {
      assert(line, "Word outside a line");
      word = {
        text: "",
        box: ["xMin", "yMin", "xMax", "yMax"].map((key) =>
          Number(attributes[key]),
        ),
      };
      assert(word.box.every(Number.isFinite), "Invalid text coordinates");
      line.push(word);
    }
  };
  parser.ontext = (text) => {
    if (word) word.text += text;
  };
  parser.onclosetag = (name) => {
    if (name === "word") {
      if (word) word.text = word.text.normalize("NFKC");
      word = undefined;
    }
    if (name === "line") line = undefined;
  };
  parser.write(xml).close();
  assert(pages.length, "PDF has no pages");
  return pages;
}
export function imageGeometry(xml) {
  const parser = sax.parser(true);
  const pages = [];
  let page;
  parser.onerror = (error) => {
    throw error;
  };
  parser.onopentag = ({ name, attributes }) => {
    if (name === "page") {
      page = [];
      pages.push(page);
    }
    if (name === "image") {
      assert(page, "Image outside a page");
      const box = ["left", "top", "width", "height"].map((key) =>
        Number(attributes[key]),
      );
      assert(
        box.every(Number.isFinite) && box[2] > 0 && box[3] > 0,
        "Invalid image coordinates",
      );
      page.push(box);
    }
  };
  parser.write(xml).close();
  return pages;
}
export function withoutFigureText(pages, figures) {
  return pages.map((page, p) => ({
    ...page,
    lines: page.lines
      .map((line) =>
        line.filter(
          (word) =>
            !(figures[p] ?? []).some(
              ([x, y, w, h]) =>
                word.box[0] >= x - 0.25 &&
                word.box[1] >= y - 0.25 &&
                word.box[2] <= x + w + 0.25 &&
                word.box[3] <= y + h + 0.25,
            ),
        ),
      )
      .filter((line) => line.length),
  }));
}

export function compareGeometry(reference, candidate, tolerance = 0.25) {
  const differences = [];
  if (reference.length !== candidate.length)
    differences.push(`Page count ${reference.length} != ${candidate.length}`);
  for (let p = 0; p < Math.min(reference.length, candidate.length); p++) {
    const a = reference[p],
      b = candidate[p];
    if (
      Math.abs(a.width - b.width) > tolerance ||
      Math.abs(a.height - b.height) > tolerance
    )
      differences.push(`Page ${p + 1}: page geometry differs`);
    if (a.lines.length !== b.lines.length)
      differences.push(
        `Page ${p + 1}: line count ${a.lines.length} != ${b.lines.length}`,
      );
    for (let l = 0; l < Math.min(a.lines.length, b.lines.length); l++) {
      const x = a.lines[l],
        y = b.lines[l];
      if (x.map((w) => w.text).join(" ") !== y.map((w) => w.text).join(" ")) {
        differences.push(
          `Page ${p + 1}, line ${l + 1}: text or line break differs`,
        );
        continue;
      }
      if (x.length !== y.length) {
        differences.push(
          `Page ${p + 1}, line ${l + 1}: word segmentation differs`,
        );
        continue;
      }
      for (let w = 0; w < x.length; w++)
        if (x[w].box.some((n, i) => Math.abs(n - y[w].box[i]) > tolerance))
          differences.push(
            `Page ${p + 1}, line ${l + 1}, word ${w + 1}: placement exceeds ${tolerance} pt`,
          );
    }
  }
  return differences;
}
// Standard local-window SSIM, with luminance in [0,255] and 8x8 windows.
export function ssim(a, b, width, height) {
  assert.equal(a.length, width * height);
  assert.equal(b.length, a.length);
  let total = 0,
    windows = 0;
  for (let y = 0; y < height; y += 8)
    for (let x = 0; x < width; x += 8) {
      let sa = 0,
        sb = 0,
        saa = 0,
        sbb = 0,
        sab = 0,
        n = 0;
      for (let dy = y; dy < Math.min(y + 8, height); dy++)
        for (let dx = x; dx < Math.min(x + 8, width); dx++) {
          const av = a[dy * width + dx],
            bv = b[dy * width + dx];
          sa += av;
          sb += bv;
          saa += av * av;
          sbb += bv * bv;
          sab += av * bv;
          n++;
        }
      const ma = sa / n,
        mb = sb / n,
        va = Math.max(0, saa / n - ma * ma),
        vb = Math.max(0, sbb / n - mb * mb),
        cov = sab / n - ma * mb;
      total +=
        ((2 * ma * mb + 6.5025) * (2 * cov + 58.5225)) /
        ((ma * ma + mb * mb + 6.5025) * (va + vb + 58.5225));
      windows++;
    }
  return total / windows;
}
export async function comparePdf(reference, candidate, output) {
  await mkdir(output, { recursive: true });
  const settings = JSON.parse(
    await readFile(
      new URL("../native/pdf/READINESS.json", import.meta.url),
      "utf8",
    ),
  ).comparison;
  const inspect = async (path) =>
    textGeometry(
      (
        await execute("pdftotext", ["-bbox-layout", path, "-"], {
          maxBuffer: 16 * 1024 * 1024,
          timeout: 120000,
        })
      ).stdout,
    );
  const a = await inspect(reference),
    b = await inspect(candidate);
  const differences = [];
  const sharp = require("sharp");
  sharp.cache(false);
  sharp.concurrency(1);
  const stage = await mkdtemp(join(tmpdir(), "publisle-parity-"));
  const scores = [];
  const figureScores = [];
  let figures;
  try {
    await execute(
      "pdftohtml",
      ["-xml", "-hidden", "-zoom", "1", reference, join(stage, "images")],
      { timeout: 120000, maxBuffer: 65536 },
    );
    figures = imageGeometry(await readFile(join(stage, "images.xml"), "utf8"));
    differences.push(
      ...compareGeometry(
        withoutFigureText(a, figures),
        withoutFigureText(b, figures),
        settings.maximumGeometryErrorPt,
      ),
    );
    for (let p = 1; p <= Math.min(a.length, b.length); p++) {
      const load = async (path, name) => {
        const prefix = join(stage, name);
        await execute(
          "pdftoppm",
          [
            "-f",
            String(p),
            "-l",
            String(p),
            "-singlefile",
            "-r",
            String(settings.rasterDpi),
            "-png",
            path,
            prefix,
          ],
          { timeout: 120000, maxBuffer: 65536 },
        );
        return sharp(`${prefix}.png`, { limitInputPixels: 16_000_000 })
          .flatten({ background: "#ffffff" })
          .greyscale()
          .raw()
          .toBuffer({ resolveWithObject: true });
      };
      const x = await load(reference, "reference"),
        y = await load(candidate, "candidate");
      if (x.info.width !== y.info.width || x.info.height !== y.info.height) {
        differences.push(`Page ${p}: rendered dimensions differ`);
        continue;
      }
      const score = ssim(x.data, y.data, x.info.width, x.info.height);
      scores.push({ page: p, ssim: score });
      if (score < settings.minimumPageSsim)
        differences.push(
          `Page ${p}: SSIM ${score} below ${settings.minimumPageSsim}`,
        );
      for (const [index, box] of (figures[p - 1] ?? []).entries()) {
        const scale = settings.rasterDpi / 72;
        const left = Math.max(0, Math.floor(box[0] * scale)),
          top = Math.max(0, Math.floor(box[1] * scale));
        const width = Math.min(x.info.width - left, Math.ceil(box[2] * scale)),
          height = Math.min(x.info.height - top, Math.ceil(box[3] * scale));
        if (width <= 0 || height <= 0) {
          differences.push(
            `Page ${p}: figure ${index + 1} is outside the page`,
          );
          continue;
        }
        const crop = (data) => {
          const bytes = Buffer.alloc(width * height);
          for (let row = 0; row < height; row++)
            data.copy(
              bytes,
              row * width,
              (top + row) * x.info.width + left,
              (top + row) * x.info.width + left + width,
            );
          return bytes;
        };
        const first = crop(x.data),
          second = crop(y.data);
        const score = ssim(first, second, width, height);
        figureScores.push({ page: p, figure: index + 1, box, ssim: score });
        if (score < settings.minimumFigureSsim)
          differences.push(
            `Page ${p}, figure ${index + 1}: SSIM ${score} below ${settings.minimumFigureSsim}`,
          );
        await sharp(first, { raw: { width, height, channels: 1 } })
          .png()
          .toFile(join(output, `page-${p}-figure-${index + 1}-reference.png`));
        await sharp(second, { raw: { width, height, channels: 1 } })
          .png()
          .toFile(join(output, `page-${p}-figure-${index + 1}-candidate.png`));
      }
      // Preserve both pages and an absolute-difference image for visual review.
      await sharp(x.data, { raw: x.info })
        .png()
        .toFile(join(output, `page-${p}-reference.png`));
      await sharp(y.data, { raw: y.info })
        .png()
        .toFile(join(output, `page-${p}-candidate.png`));
      const diff = Buffer.alloc(x.data.length);
      for (let i = 0; i < diff.length; i++)
        diff[i] = Math.abs(x.data[i] - y.data[i]);
      await sharp(diff, { raw: x.info })
        .png()
        .toFile(join(output, `page-${p}-difference.png`));
    }
  } finally {
    await rm(stage, { recursive: true, force: true });
  }
  const digest = async (path) =>
    createHash("sha256")
      .update(await readFile(path))
      .digest("hex");
  const report = {
    passed: differences.length === 0,
    referenceSha256: await digest(reference),
    candidateSha256: await digest(candidate),
    settings,
    referenceGeometry: a,
    candidateGeometry: b,
    scores,
    figureScores,
    figuresPassed: figureScores.every(
      (figure) => figure.ssim >= settings.minimumFigureSsim,
    ),
    differences,
  };
  await writeFile(
    join(output, "comparison.json"),
    JSON.stringify(report, null, 2),
  );
  return report;
}
if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  assert.equal(
    process.argv.length,
    5,
    "Usage: node tools/native-pdf-parity.mjs REFERENCE.pdf CANDIDATE.pdf OUTPUT_DIRECTORY",
  );
  const report = await comparePdf(...process.argv.slice(2));
  console.log(
    JSON.stringify({
      passed: report.passed,
      differences: report.differences.length,
      scores: report.scores,
    }),
  );
  if (!report.passed) process.exitCode = 1;
}
