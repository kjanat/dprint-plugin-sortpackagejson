import { $, env } from "bun";
import { join } from "node:path";
import { defineConfig } from "tsdown";

const repoRoot = join(import.meta.dir, "../..");
const wasmFile = join(
	repoRoot,
	"target/wasm32-unknown-unknown/wasm-release",
	"dprint_plugin_sortpackagejson.wasm",
);

export default defineConfig({
	entry: "./src/index.ts",
	exports: {
		enabled: true,
		packageJson: true,
		customExports(exports) {
			exports["./wasm"] = `./dist/plugin.wasm`;
			exports["./schema"] = `./dist/schema.json`;
			return exports;
		},
	},
	dts: { cjsReexport: true },
	clean: true,
	target: "esnext",
	format: "es", // ["es", "cjs"],
	copy: [{ from: wasmFile, rename: "plugin.wasm" }],
	hooks: {
		async "build:before"(ctx) {
			if (ctx.options.format !== "es") return;
			Promise.all([
				await $`just wasm`.cwd(repoRoot),
				await $`just schema ${import.meta.dir}/dist/schema.json`,
			]);
		},
		async "build:done"() {
			syncVersions().catch((error) => {
				console.error("Error syncing versions after build:", error);
			});
		},
	},
	onSuccess: "just fmt",
});

async function syncVersions() {
	const { default: cargo } = await import("../../Cargo.toml", {
		with: { type: "toml" },
	});
	const pkgVersion = env.npm_package_version;
	const cargoVersion: string = typeof cargo.package.version === "string" && /^\d+\.\d+\.\d+/.test(cargo.package.version)
		? cargo.package.version
		: (() => {
			console.warn(
				`Cargo.toml version (${cargo.package.version}) is not a valid semver version. Defaulting to 0.0.0.`,
			);
			return "0.0.0";
		})();

	if (pkgVersion === undefined) {
		console.warn(`package.json version is undefined. Setting it to ${cargoVersion}.`);
		await $`bun pm pkg set version="$VERSION"`.env({ ...process.env, VERSION: cargoVersion });
	} else if (pkgVersion !== cargoVersion) {
		console.log(`Updating package.json version from ${pkgVersion} to ${cargoVersion}`);
		await $`bun pm pkg set version="$VERSION"`.env({ ...process.env, VERSION: cargoVersion });
	} else {
		console.log(`package.json version (${pkgVersion}) is up to date with Cargo.toml version.`);
	}
}
