import { fileURLToPath } from "node:url";

/** @returns {string} The path to the Wasm module. */
function getPath(): string {
	return fileURLToPath(import.meta.resolve("@kjanat/dprint-plugin-sortpackagejson/wasm"));
}

export { getPath };
