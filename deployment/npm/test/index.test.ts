import { describe, expect, test } from "bun:test";
import fs from "node:fs";

import { createFromBuffer } from "@dprint/formatter";
import { getPath } from "../src/index.ts";

const pluginPath = getPath();
const pluginBuffer = fs.readFileSync(pluginPath);
const formatter = createFromBuffer(pluginBuffer);

describe("sortpackagejson plugin", () => {
	// This is a basic test to ensure the plugin is working. The plugin should be tested more thoroughly in Rust.
	test("sorts package.json", () => {
		const result = formatter.formatText({
			filePath: "package.json",
			fileText: `{
	"version": "1.0.0",
	"name": "demo"
}\n`,
		});
		const expectedOutput = `{
	"name": "demo",
	"version": "1.0.0"
}\n`;
		expect(result).toBe(expectedOutput);
	});
});
