import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../../../../..",
);
const retiredCustomerName = ["[Tt]he", "sen"].join("");
const retiredCustomerBrand = ["THE", "SEN"].join("");
const result = spawnSync(
  "rg",
  [
    "-l",
    `${retiredCustomerName}(?:[-_ ](?:[Oo]utbound)|\\.ctox\\.dev)|\\b${retiredCustomerBrand}\\b`,
    "src",
  ],
  { cwd: repositoryRoot, encoding: "utf8" },
);

try {
  assert.ok(
    result.status === 0 || result.status === 1,
    `customer identity scan failed: ${JSON.stringify({
      status: result.status,
      signal: result.signal,
      error: result.error?.message,
      stderr: result.stderr?.trim(),
    })}`,
  );
  assert.equal(
    result.stdout.trim(),
    "",
    `active source must not contain the retired customer identity:\n${result.stdout.trim()}`,
  );
  console.log("customer identifier inventory smoke OK");
} catch (error) {
  // run-all prints a short output tail; retain the cause instead of a stack tail.
  console.error(error.message);
  process.exitCode = 1;
}
