import path from "node:path";

/** Gets the path to the Wasm module. */
export function getPath(): string {
	return path.join(__dirname, "../plugin.wasm");
}
