/** Explicit checksum-pinned veraPDF installation; requires Java, never Python. */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { mkdtemp, writeFile, rm, lstat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";
import { pathToFileURL } from "node:url";
import { unzipSync } from "fflate";
export const VERSION = "1.28.2";
export const SHA256 =
  "d1693a5f0bf0997180f6d97e8a5568b0cf39e27eee338d439c8adb58435c1e89";
export const URL = `https://software.verapdf.org/releases/1.28/verapdf-greenfield-${VERSION}-installer.zip`;
const xml = (value) =>
  value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&apos;");
export function installerJar(bytes) {
  assert.equal(
    createHash("sha256").update(bytes).digest("hex"),
    SHA256,
    "veraPDF installer checksum mismatch",
  );
  const name = `verapdf-greenfield-${VERSION}/verapdf-izpack-installer-${VERSION}.jar`;
  const archive = unzipSync(bytes, {
    filter: (file) =>
      file.name === name && file.originalSize <= 64 * 1024 * 1024,
  });
  assert(archive[name]?.length, "Missing or oversized veraPDF installer JAR");
  return archive[name];
}
export async function install(destination) {
  destination = resolve(destination);
  await assert.rejects(
    lstat(destination),
    { code: "ENOENT" },
    "Installation destination already exists",
  );
  const stage = await mkdtemp(join(tmpdir(), "publisle-verapdf-"));
  try {
    const response = await fetch(URL, { signal: AbortSignal.timeout(120000) });
    assert(
      response.ok && response.body,
      `veraPDF download failed: ${response.status}`,
    );
    const parts = [];
    let size = 0;
    for await (const part of response.body) {
      size += part.length;
      if (size > 32 * 1024 * 1024) {
        await response.body.cancel().catch(() => {
          /* The iterator may still hold the reader lock. */
        });
        throw new Error("veraPDF installer exceeds 32 MiB");
      }
      parts.push(part);
    }
    const jar = join(stage, "installer.jar");
    await writeFile(jar, installerJar(Buffer.concat(parts)), { flag: "wx" });
    const prefix = "com.izforge.izpack.panels.";
    const packs = [
      "veraPDF GUI",
      "veraPDF Mac and *nix Scripts",
      "veraPDF Validation model",
      "veraPDF Documentation",
      "veraPDF Sample Plugins",
    ];
    const config = `<?xml version="1.0" encoding="utf-8"?><AutomatedInstallation langpack="eng"><${prefix}htmlhello.HTMLHelloPanel id="welcome"/><${prefix}target.TargetPanel id="install_dir"><installpath>${xml(destination)}</installpath></${prefix}target.TargetPanel><${prefix}packs.PacksPanel id="sdk_pack_select">${packs.map((name, index) => `<pack index="${index}" name="${xml(name)}" selected="${index < 2}"/>`).join("")}</${prefix}packs.PacksPanel><${prefix}install.InstallPanel id="install"/><${prefix}finish.FinishPanel id="finish"/></AutomatedInstallation>`;
    const path = join(stage, "install.xml");
    await writeFile(path, config, { flag: "wx" });
    const execute = promisify(execFile);
    await execute("java", ["-jar", jar, path], {
      timeout: 120000,
      maxBuffer: 4 * 1024 * 1024,
    });
    await execute(join(destination, "verapdf"), ["--version"], {
      timeout: 30000,
    });
  } finally {
    await rm(stage, { recursive: true, force: true });
  }
}
if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  assert.equal(
    process.argv.length,
    3,
    "Usage: node tools/install-verapdf.mjs NEW_DIRECTORY",
  );
  await install(process.argv[2]);
}
