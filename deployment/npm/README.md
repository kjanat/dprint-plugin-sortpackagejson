# @kjanat/dprint-plugin-sortpackagejson

npm distribution of [`dprint-plugin-sortpackagejson`], which is an adaptation of
the [`sort-package-json`] npm package.

## Install

```bash
npm install @dprint/formatter @kjanat/dprint-plugin-sortpackagejson
```

Or install straight from dprint:

```bash
dprint add kjanat/sortpackagejson
```

Use it in `dprint.json` like this:

```jsonc
{
  "plugins": [
    "./node_modules/@kjanat/dprint-plugin-sortpackagejson/plugin.wasm",
  ],
  "sortPackageJson": {},
}
```

## Usage

Programmatic consumers can resolve the wasm path in a few ways.

With `@dprint/formatter`:

```js
import { readFileSync } from "node:fs";
import { createFromBuffer } from "@dprint/formatter";
import { getPath } from "@kjanat/dprint-plugin-sortpackagejson";

const formatter = createFromBuffer(readFileSync(getPath()));

const formattedText = formatter.formatText({
  filePath: "package.json",
  fileText: '{"name":"test","version":"1.0.0"}',
});

console.log(formattedText);
```

Or resolve the wasm module directly with `import.meta.resolve`:

```js
import { createFromWasmModule } from "@dprint/formatter";
import { file, fileURLToPath } from "bun";

const wasmModule = await WebAssembly.compile(
  await file(
    fileURLToPath(
      import.meta.resolve("@kjanat/dprint-plugin-sortpackagejson/wasm"),
    ),
  ).arrayBuffer(),
);
const formatter = createFromWasmModule(wasmModule);
```

And streaming is also supported:

```js
import { createStreaming } from "@dprint/formatter";

const formatter = await createStreaming(
  fetch(import.meta.resolve("@kjanat/dprint-plugin-sortpackagejson/wasm")),
);
```

## Links

- Repository: <https://github.com/kjanat/dprint-plugin-sortpackagejson>
- Issues: <https://github.com/kjanat/dprint-plugin-sortpackagejson/issues>

[`dprint-plugin-sortpackagejson`]: https://github.com/kjanat/dprint-plugin-sortpackagejson
[`sort-package-json`]: https://npm.im/sort-package-json
