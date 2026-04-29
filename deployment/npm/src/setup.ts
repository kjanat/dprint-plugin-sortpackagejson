import { copyFileSync, existsSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

const repoRoot = path.join(import.meta.dirname, "../../..");
const pkgRoot = path.join(import.meta.dirname, "..");

function parsePackageJson(text: string) {
	const value: unknown = JSON.parse(text);
	if (
		typeof value !== "object"
		|| value === null
		|| !("version" in value)
		|| typeof value.version !== "string"
	) {
		throw new Error("package.json must contain a string version");
	}

	return value;
}

const args = Bun.argv.slice(2);
const wasmPath = path.join(
	repoRoot,
	"target/wasm32-unknown-unknown/wasm-release/dprint_plugin_sortpackagejson.wasm",
);
const packageJsonPath = path.join(pkgRoot, "package.json");
const cargoTomlPath = path.join(repoRoot, "Cargo.toml");
const publishedWasmPath = path.join(pkgRoot, "plugin.wasm");

if (!existsSync(wasmPath)) {
	throw new Error(`Missing wasm build at ${wasmPath}. Run \`just wasm\` first.`);
}

copyFileSync(wasmPath, publishedWasmPath);

if (args.length === 0) {
	process.exit(0);
}

const packageJson = parsePackageJson(readFileSync(packageJsonPath, "utf8"));
if (args[0] === "sync-version") {
	const cargoTomlText = readFileSync(cargoTomlPath, "utf8");
	const versionMatch = cargoTomlText.match(/^version\s*=\s*"([^"]+)"/m);
	if (!versionMatch) {
		throw new Error("Could not find version in Cargo.toml");
	}
	packageJson.version = versionMatch[1];
} else {
	packageJson.version = args[0];
}

writeFileSync(packageJsonPath, `${JSON.stringify(packageJson, undefined, "\t")}\n`);
