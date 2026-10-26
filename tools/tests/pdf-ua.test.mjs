import { test } from "node:test";
import assert from "node:assert/strict";
import { assertCompliant } from "../validate-pdf-ua.mjs";
const valid =
  '<report><jobs><job><validationReport jobEndStatus="normal" profileName="PDF/UA-2 + Tagged PDF validation profile" isCompliant="true"><details failedRules="0" failedChecks="0"/></validationReport></job></jobs><batchSummary totalJobs="1" failedToParse="0" encrypted="0" outOfMemory="0" veraExceptions="0"><validationReports compliant="1" nonCompliant="0" failedJobs="0">1</validationReports></batchSummary></report>';
test("requires one complete, matching, successful veraPDF job", () => {
  assertCompliant(valid);
  assertCompliant(valid.replace("PDF/UA-2 + Tagged PDF", "PDF/UA-1"), "ua1");
  const invalid = [
    "not XML",
    "<report/>",
    valid.replace("PDF/UA-2", "PDF/A-2B"),
    valid.replace('isCompliant="true"', 'isCompliant="false"'),
    valid.replace('jobEndStatus="normal"', 'jobEndStatus="aborted"'),
    valid.replace('jobEndStatus="normal"', ""),
    valid.replace("</job>", "</job><job/>"),
    valid.replace('failedRules="0"', 'failedRules="1"'),
    valid.replace('failedChecks="0"', 'failedChecks="1"'),
    valid.replace('totalJobs="1"', 'totalJobs="0"'),
    valid.replace('failedToParse="0"', 'failedToParse="1"'),
    valid.replace('veraExceptions="0"', 'veraExceptions="1"'),
    valid.replace('failedJobs="0"', 'failedJobs="1"'),
    valid.replace('nonCompliant="0"', 'nonCompliant="1"'),
    valid.replace('<details failedRules="0" failedChecks="0"/>', ""),
    valid.replace(
      "</job>",
      '</job><job><validationReport isCompliant="true"/></job>',
    ),
    '<!DOCTYPE report [<!ENTITY x SYSTEM "file:///etc/passwd">]>' + valid,
    valid + "<report/>",
    valid.replace("</report>", ""),
    valid.replace("</jobs>", "</jobs><jobs/>"),
  ];
  for (const report of invalid)
    assert.throws(() => assertCompliant(report), report);
});
