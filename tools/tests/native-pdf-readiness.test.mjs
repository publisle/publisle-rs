import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createHash } from "node:crypto";
import { verifyReadiness } from "../native-pdf-readiness.mjs";
test("release gate rejects missing, stale, corrupt and incomplete readiness evidence", async () => {
  const directory = await mkdtemp(join(tmpdir(), "publisle-readiness-"));
  try {
    const matrix = {
      schemaVersion: 1,
      profiles: ["article"],
      gates: [{ id: "svg-fonts" }],
    };
    const sourceCommit = "a".repeat(40);
    const bytes = Buffer.from("independent validation fixture");
    await writeFile(join(directory, "report.txt"), bytes);
    const artifact = {
      path: "report.txt",
      sha256: createHash("sha256").update(bytes).digest("hex"),
    };
    const evidence = {
      schemaVersion: 1,
      image: `sha256:${"b".repeat(64)}`,
      sourceCommit,
      profiles: ["article"],
      gates: [{ id: "svg-fonts", passed: true, artifacts: [artifact] }],
      distribution: {
        platforms: [
          "darwin-arm64",
          "darwin-x64",
          "linux-arm64",
          "linux-x64",
          "win32-x64",
        ],
      },
      performance: {
        aggregateCpu: 0.5,
        aggregateMemoryMiB: 512,
        swapMiB: 0,
        results: [
          {
            template: "article",
            warmups: 5,
            uncachedSamples: 30,
            uncachedP95Ms: 1000,
            observations: Array(30).fill(1000),
          },
        ],
      },
    };
    assert.equal(
      await verifyReadiness(matrix, evidence, directory, sourceCommit),
      true,
    );
    // Legacy layout comparisons are optional diagnostics, including failures.
    assert.equal(
      await verifyReadiness(
        matrix,
        { ...evidence, parity: { passed: false, figuresPassed: false } },
        directory,
        sourceCommit,
      ),
      true,
    );
    await assert.rejects(
      verifyReadiness(matrix, evidence, directory, "c".repeat(40)),
      /different source/,
    );
    await assert.rejects(
      verifyReadiness(
        matrix,
        { ...evidence, gates: [] },
        directory,
        sourceCommit,
      ),
      /incomplete/,
    );
    await assert.rejects(
      verifyReadiness(
        matrix,
        {
          ...evidence,
          gates: [{ id: "svg-fonts", passed: false, artifacts: [artifact] }],
        },
        directory,
        sourceCommit,
      ),
      /Unverified/,
    );
    await assert.rejects(
      verifyReadiness(
        matrix,
        {
          ...evidence,
          performance: {
            ...evidence.performance,
            results: [{ ...evidence.performance.results[0], uncachedP95Ms: 1 }],
          },
        },
        directory,
        sourceCommit,
      ),
      /p95/,
    );
    await writeFile(join(directory, "report.txt"), "tampered");
    await assert.rejects(
      verifyReadiness(matrix, evidence, directory, sourceCommit),
      /Evidence changed/,
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
