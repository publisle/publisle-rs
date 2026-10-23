/** Immutable collision preflight and resumable registry reconciliation. */
import assert from "node:assert/strict";
export async function publishPackages(packages, io) {
  // Check the entire batch before any mutation, including immutable collisions.
  for (const p of packages) {
    const old = await io.lookup(p);
    if (old)
      assert.equal(
        old.dist.integrity,
        p.integrity,
        `Different published bytes: ${p.name}`,
      );
  }
  for (const p of packages) {
    const key = `${p.name}@${p.version}`;
    let m = await io.lookup(p);
    if (!m && !io.accepted.has(key)) {
      try {
        await io.upload(p);
      } catch (error) {
        if (!/\bE409\b|previously staged version/u.test(String(error)))
          throw error;
      }
      io.accepted.add(key);
      await io.save(key);
    }
    for (let n = 0; !m && n < (io.attempts ?? 180); n++) {
      await io.wait();
      m = await io.lookup(p);
    }
    assert(
      m,
      `Accepted upload is still processing: ${key}. Resume this run; do not rebuild or republish.`,
    );
    assert.equal(
      m.dist.integrity,
      p.integrity,
      `Different published bytes: ${p.name}`,
    );
  }
}
