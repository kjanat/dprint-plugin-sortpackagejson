import { join } from "node:path";

/** @returns {string} The path to the Wasm module. */
function getPath(): string {
	return join(import.meta.dirname, "./plugin.wasm");
}

export { getPath };
