import { createStreaming } from "@dprint/formatter";
import { describe, expect, test } from "bun:test";

const formatter = await createStreaming(
	fetch(import.meta.resolve("@kjanat/dprint-plugin-sortpackagejson/wasm")),
);

describe("sortpackagejson plugin", () => {
	// This is a basic test to ensure the plugin is working.
	test("sorts package.json", () => {
		const result = formatter.formatText({
			filePath: "package.json",
			fileText: `{ "version": "1.0.0", "name": "test" }`,
		});
		const expectedOutput = `{ "name": "test", "version": "1.0.0" }`;
		expect(result).toBe(expectedOutput);
	});
});
