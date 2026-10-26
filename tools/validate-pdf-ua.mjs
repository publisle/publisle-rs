/** Strict veraPDF report checking. A requested standard is never a validation result. */
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import sax from "sax";

export function assertCompliant(xml, flavour = "ua2") {
  assert(["ua1", "ua2"].includes(flavour), "Unsupported PDF/UA flavour");
  assert(Buffer.byteLength(xml) <= 8 * 1024 * 1024, "Oversized veraPDF report");
  const parser = sax.parser(true, { strictEntities: true });
  let root;
  const stack = [];
  let count = 0;
  parser.ondoctype = () => {
    throw new Error("DTDs are not allowed in validation reports");
  };
  parser.onerror = (error) => {
    throw error;
  };
  parser.onopentag = ({ name, attributes }) => {
    assert(
      stack.length < 64 && ++count <= 100000,
      "Validation report exceeds limits",
    );
    const node = { name, attributes, children: [] };
    if (stack.length) stack.at(-1).children.push(node);
    else {
      assert(!root, "Multiple report roots");
      root = node;
    }
    stack.push(node);
  };
  parser.onclosetag = () => {
    stack.pop();
  };
  parser.write(String(xml)).close();
  assert(root?.name === "report", "Expected a veraPDF report");
  const only = (node, name) => {
    const matches = node.children.filter((child) => child.name === name);
    assert.equal(matches.length, 1, `Expected exactly one ${name}`);
    return matches[0];
  };
  const equals = (node, expected) => {
    for (const [key, value] of Object.entries(expected))
      assert.equal(node.attributes[key], value, `Invalid ${node.name}.${key}`);
  };
  const job = only(only(root, "jobs"), "job");
  const report = only(job, "validationReport");
  equals(report, {
    jobEndStatus: "normal",
    profileName:
      flavour === "ua2"
        ? "PDF/UA-2 + Tagged PDF validation profile"
        : "PDF/UA-1 validation profile",
    isCompliant: "true",
  });
  equals(only(report, "details"), { failedRules: "0", failedChecks: "0" });
  const summary = only(root, "batchSummary");
  equals(summary, {
    totalJobs: "1",
    failedToParse: "0",
    encrypted: "0",
    outOfMemory: "0",
    veraExceptions: "0",
  });
  equals(only(summary, "validationReports"), {
    compliant: "1",
    nonCompliant: "0",
    failedJobs: "0",
  });
}

export async function validatePdf(
  pdf,
  { validator = "verapdf", flavour = "ua2", report } = {},
) {
  assert(report, "A report path is required");
  assert(["ua1", "ua2"].includes(flavour), "Unsupported PDF/UA flavour");
  const execute = promisify(execFile);
  let result;
  try {
    result = await execute(
      validator,
      ["--format", "xml", "--flavour", flavour, resolve(pdf)],
      { timeout: 120000, maxBuffer: 8 * 1024 * 1024 },
    );
  } catch (error) {
    await writeFile(report, error.stdout ?? "");
    throw error;
  }
  await writeFile(report, result.stdout);
  assertCompliant(result.stdout, flavour);
  return result.stdout;
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  const [pdf, ...args] = process.argv.slice(2);
  assert(
    pdf && args.length % 2 === 0,
    "Usage: node tools/validate-pdf-ua.mjs PDF --report PATH [--validator PATH] [--flavour ua1|ua2]",
  );
  const options = {};
  for (let i = 0; i < args.length; i += 2) {
    assert(
      ["--report", "--validator", "--flavour"].includes(args[i]),
      `Unknown option ${args[i]}`,
    );
    assert(!(args[i].slice(2) in options), "Duplicate option");
    options[args[i].slice(2)] = args[i + 1];
  }
  await validatePdf(pdf, options);
  console.log(
    `PDF/${(options.flavour ?? "ua2").toUpperCase()} rules passed: ${pdf}`,
  );
}
