# @kjanat/dprint-plugin-sortpackagejson

npm distribution of [`dprint-plugin-sortpackagejson`](https://github.com/kjanat/dprint-plugin-sortpackagejson).

Use it in `dprint.json` like this:

```jsonc
{
  "plugins": [
    "./node_modules/@kjanat/dprint-plugin-sortpackagejson/plugin.wasm",
  ],
  "sortPackageJson": {},
}
```

Programmatic consumers can resolve the wasm path via `getPath()`:

```js
const { getPath } = require("@kjanat/dprint-plugin-sortpackagejson");
```
