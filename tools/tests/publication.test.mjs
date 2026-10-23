import { test } from "node:test";
import assert from "node:assert/strict";
import { publishPackages } from "../publication.mjs";
const pkg = { name: "native", version: "0.3.1", integrity: "correct" };
const defaults = {
  accepted: new Set(),
  save: async () => {},
  wait: async () => {},
  attempts: 3,
};
test("preflights every collision before uploading any package", async () => {
  let uploads = 0;
  await assert.rejects(
    publishPackages([pkg, { ...pkg, name: "second" }], {
      ...defaults,
      lookup: async (p) =>
        p.name === "second" ? { dist: { integrity: "wrong" } } : null,
      upload: async () => {
        uploads++;
      },
    }),
    /Different published bytes/,
  );
  assert.equal(uploads, 0);
});
test("resumes persisted accepted uploads without republishing", async () => {
  let uploads = 0,
    lookups = 0;
  await publishPackages([pkg], {
    ...defaults,
    accepted: new Set(["native@0.3.1"]),
    lookup: async () =>
      ++lookups > 3 ? { dist: { integrity: "correct" } } : null,
    upload: async () => {
      uploads++;
    },
  });
  assert.equal(uploads, 0);
});
test("persists staged uploads and waits for registry confirmation", async () => {
  let uploads = 0,
    lookups = 0,
    saves = 0;
  const accepted = new Set();
  await publishPackages([pkg], {
    ...defaults,
    accepted,
    lookup: async () =>
      ++lookups > 3 ? { dist: { integrity: "correct" } } : null,
    upload: async () => {
      uploads++;
      throw new Error("npm E409 previously staged version");
    },
    save: async () => {
      saves++;
    },
  });
  assert.equal(uploads, 1);
  assert.equal(saves, 1);
  assert(accepted.has("native@0.3.1"));
});
test("pending uploads stop with a recovery instruction instead of republishing", async () => {
  let uploads = 0;
  await assert.rejects(
    publishPackages([pkg], {
      ...defaults,
      accepted: new Set(["native@0.3.1"]),
      lookup: async () => null,
      upload: async () => {
        uploads++;
      },
    }),
    /still processing/,
  );
  assert.equal(uploads, 0);
});
