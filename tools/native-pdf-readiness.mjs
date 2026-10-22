/** One complete release: incomplete, stale, or missing evidence blocks publication. */
import assert from "node:assert/strict";
import { readFile, realpath } from "node:fs/promises";
import { resolve, isAbsolute, relative, sep } from "node:path";
import { pathToFileURL } from "node:url";
import { createHash } from "node:crypto";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
export async function verifyReadiness(
  matrix,
  evidence,
  directory,
  expectedCommit,
) {
  assert.equal(matrix.schemaVersion, 1);
  assert.equal(evidence.schemaVersion, 1);
  assert.equal(
    evidence.sourceCommit,
    expectedCommit,
    "Evidence is for a different source commit",
  );
  directory = await realpath(directory);
  assert(
    /^sha256:[a-f0-9]{64}$/u.test(evidence.image),
    "Missing immutable candidate image",
  );
  assert(/^[a-f0-9]{40}$/u.test(evidence.sourceCommit), "Missing sourceCommit");
  assert.deepEqual(
    [...evidence.profiles].sort(),
    [...matrix.profiles].sort(),
    "Every profile must be verified",
  );
  assert.equal(
    new Set(evidence.gates?.map((g) => g.id)).size,
    matrix.gates.length,
    "Readiness evidence is incomplete",
  );
  for (const gate of matrix.gates) {
    const result = evidence.gates.find((entry) => entry.id === gate.id);
    assert(
      result?.passed === true &&
        Array.isArray(result.artifacts) &&
        result.artifacts.length > 0,
      `Unverified native PDF gate: ${gate.id}`,
    );
    for (const artifact of result.artifacts) {
      assert(
        typeof artifact.path === "string" &&
          !isAbsolute(artifact.path) &&
          /^[a-f0-9]{64}$/u.test(artifact.sha256),
        "Invalid evidence artifact",
      );
      const path = await realpath(resolve(directory, artifact.path)),
        local = relative(directory, path);
      assert(
        local !== ".." && !local.startsWith(`..${sep}`) && !isAbsolute(local),
        "Evidence escapes its directory",
      );
      assert.equal(
        createHash("sha256")
          .update(await readFile(path))
          .digest("hex"),
        artifact.sha256,
        `Evidence changed: ${artifact.path}`,
      );
    }
  }
  assert.deepEqual(
    evidence.distribution?.platforms?.slice().sort(),
    ["darwin-arm64", "darwin-x64", "linux-arm64", "linux-x64", "win32-x64"],
    "Every native platform must pass clean installation",
  );
  assert.equal(evidence.performance?.aggregateCpu, 0.5);
  assert.equal(evidence.performance?.aggregateMemoryMiB, 512);
  assert.equal(evidence.performance?.swapMiB, 0);
  for (const profile of matrix.profiles) {
    const row = evidence.performance.results?.find(
      (result) => result.template === profile,
    );
    assert(
      row &&
        row.warmups >= 5 &&
        Array.isArray(row.observations) &&
        row.observations.length >= 30 &&
        row.uncachedSamples === row.observations.length &&
        row.observations.every((value) => Number.isFinite(value) && value >= 0),
      `Unverified performance: ${profile}`,
    );
    const sorted = [...row.observations].sort((a, b) => a - b);
    const p95 = sorted[Math.ceil(sorted.length * 0.95) - 1];
    assert(
      p95 < 5000 && Math.abs(p95 - row.uncachedP95Ms) < 0.000001,
      `Incorrect p95 for ${profile}`,
    );
  }
  return true;
}
if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  assert.equal(
    process.argv.length,
    3,
    "Usage: node tools/native-pdf-readiness.mjs EVIDENCE_DIRECTORY",
  );
  const directory = resolve(process.argv[2]);
  const matrix = JSON.parse(
    await readFile(
      new URL("../native/pdf/READINESS.json", import.meta.url),
      "utf8",
    ),
  );
  const evidence = JSON.parse(
    await readFile(resolve(directory, "readiness.json"), "utf8"),
  );
  const { stdout } = await promisify(execFile)("git", ["rev-parse", "HEAD"], {
    cwd: resolve(import.meta.dirname, ".."),
  });
  await verifyReadiness(matrix, evidence, directory, stdout.trim());
  console.log(
    `All ${matrix.gates.length} native PDF release gates passed for ${evidence.sourceCommit}`,
  );
}
