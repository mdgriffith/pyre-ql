#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const localBinary = join(__dirname, "..", "vendor", "pyre");

if (!existsSync(localBinary)) {
  console.error(
    "Pyre binary not found in this npm package yet. Binary distribution wiring is still in progress."
  );
  process.exit(1);
}

const result = spawnSync(localBinary, process.argv.slice(2), {
  stdio: "inherit",
});

if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}

process.exit(result.status ?? 0);
