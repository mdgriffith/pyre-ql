import initWasm from "./wasm/pyre_wasm.js";

/**
 * Initialize Pyre WASM module.
 *
 * @param wasmPath - Optional path/URL to a wasm file. If omitted, the bundled pyre-wasm package default is used.
 * @example
 * ```typescript
 * import { init } from "pyre-wasm/server";
 * await init();
 * ```
 */
export async function init(wasmPath?: string): Promise<void> {
    try {
        if (wasmPath) {
            await initWasm(wasmPath);
            return;
        }

        await initWasm();
    } catch (error: any) {
        throw new Error(`Failed to initialize Pyre WASM: ${error.message}`);
    }
}
