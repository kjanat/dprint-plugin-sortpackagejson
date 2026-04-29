import { join } from "node:path";
import { defineConfig } from "tsdown";

const wasmFile = join(
	import.meta.dirname,
	"../../target/wasm32-unknown-unknown/wasm-release",
	"dprint_plugin_sortpackagejson.wasm",
);

export default defineConfig({
	entry: "./src/index.ts",
	exports: {
		enabled: true,
		packageJson: true,
		customExports(exports) {
			exports["./wasm"] = `./dist/plugin.wasm`;
			return exports;
		},
	},
	clean: true,
	target: "esnext",
	format: "esm",
	shims: true,
	copy: [
		{ from: wasmFile, rename: "plugin.wasm" },
	],
	hooks: {},
});
