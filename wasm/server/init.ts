import { readFileSync } from "fs";
import { join } from "path";
import * as wasmModule from "../pkg/pyre_wasm.js";

/**
 * Initialize Pyre WASM module.
 * Uses the imported WASM module and automatically finds the WASM file in the pkg directory.
 * 
 * @param wasmPath - Optional path to the WASM file. If not provided, will look for it in the pkg directory.
 * @example
 * ```typescript
 * import { initPyre } from "pyre-wasm/server";
 * await initPyre();
 * ```
 */
export async function initPyre(wasmPath?: string): Promise<void> {
    // Default path assumes pyre_wasm_bg.wasm is in the pkg directory (same directory as pyre_wasm.js)
    // This can be overridden if the WASM file is in a different location
    const defaultPath = wasmPath || join(__dirname, "..", "pkg", "pyre_wasm_bg.wasm");
    const path = wasmPath || defaultPath;

    try {
        const wasmBuffer = readFileSync(path);
        await wasmModule.default({ wasm: wasmBuffer });
    } catch (error: any) {
        throw new Error(`Failed to initialize Pyre WASM: ${error.message}. Make sure the WASM file exists at ${path}`);
    }
}
