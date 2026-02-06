import { rmSync } from "node:fs";
import { join } from "node:path";

const artifactsDir = join(process.cwd(), ".artifacts");
rmSync(artifactsDir, { recursive: true, force: true });
console.log("Removed .artifacts");
