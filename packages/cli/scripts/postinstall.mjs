import { existsSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const binaryPath = join(root, "vendor", "pyre");

if (!existsSync(binaryPath)) {
  console.warn(
    "[pyre] No bundled binary found yet. This is expected during migration; binary download/install wiring is next."
  );
}
