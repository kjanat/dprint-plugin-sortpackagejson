# dprint-plugin-sortpackagejson

Rust port of [`sort-package-json`] as a [dprint] plugin.

Sorts a `package.json` into the canonical key order used across the npm
ecosystem: lifecycle scripts grouped pre/main/post, dependencies
alphabetized, exports/conditions ordered correctly, and the rest.

## Install

```sh
dprint add kjanat/sortpackagejson
```

Or install from npm:

```sh
npm install @kjanat/dprint-plugin-sortpackagejson
```

Then reference the packaged wasm file in `dprint.json`:

```jsonc
{
  "plugins": [
    "./node_modules/@kjanat/dprint-plugin-sortpackagejson/plugin.wasm",
  ],
  "sortPackageJson": {},
}
```

Or manually in your `dprint.json`:

```jsonc
{
  "plugins": ["https://plugins.dprint.dev/kjanat/sortpackagejson-x.y.z.wasm"],
  "sortPackageJson": {},
}
```

The plugin only formats files literally named `package.json`; everything
else stays with `dprint-plugin-json`.

Programmatic consumers can resolve the wasm path via
`require("@kjanat/dprint-plugin-sortpackagejson").getPath()`.

## Configuration

| Key                | Type                           | Default          | Description                                                                                         |
| ------------------ | ------------------------------ | ---------------- | --------------------------------------------------------------------------------------------------- |
| `sortOrder`        | `string[]`                     | `[]`             | Custom top-level key order; empty falls back to the canonical [`sort-package-json`](#parity) order. |
| `sortDependencies` | `boolean`                      | `true`           | Alphabetize entries inside dependency objects.                                                      |
| `sortScripts`      | `boolean`                      | `true`           | Apply pre/main/post grouping + colon-namespace handling + npm-run-all chain detection.              |
| `sortNested`       | `boolean`                      | `true`           | Apply nested-section sort rules (engines, exports, eslintConfig, prettier, workspaces, pnpm, ...).  |
| `unknownKeys`      | `"alphabetical" \| "preserve"` | `"alphabetical"` | How to order top-level keys not present in the canonical list.                                      |
| `packageManager`   | `"auto" \| "npm" \| "other"`   | `"auto"`         | Which package manager's dependency ordering to use. See [Parity](#parity).                          |

The IDE-autocomplete schema for these options lives at [`schema.json`]
and is regenerated from the Rust `Configuration` struct via `schemars`;
drift is enforced by `tests/schema_in_sync.rs`.

## Parity

Behavioral target: `sort-package-json` **v4.0.0**. Key ordering is
byte-identical to upstream for the fixtures in `tests/fixtures/`, including
`parity-4.0.json`, which exercises `wireit`, locale-aware dependency
ordering, semver-ordered `pnpm.overrides` and ESLint rule ordering.

Two things differ deliberately:

- **Package manager detection.** Upstream picks npm's locale-aware
  comparator over yarn/pnpm's plain one partly by looking for `yarn.lock`,
  `.yarnrc.yml`, `pnpm-lock.yaml` or `pnpm-workspace.yaml` on disk. A wasm
  plugin is sandboxed and cannot do that, so `"auto"` uses every
  _in-document_ signal upstream uses — `packageManager`,
  `devEngines.packageManager`, a `pnpm` key, `engines.npm` — and otherwise
  defaults to npm, the same fallback upstream lands on when it finds no lock
  file. A yarn or pnpm project whose `package.json` carries none of those
  signals should set `"packageManager": "other"`.
- **Unparseable version ranges.** `pnpm.overrides` keys are ordered by the
  minimum version each range admits, matching upstream. node-semver _throws_
  on ranges it cannot parse (`workspace:*`, `npm:foo@1.2.3`); a formatter
  must not, so those fall back to comparing the range text.

Layout is not a parity target: upstream re-serialises with
`JSON.stringify`, which expands every object and array. This plugin keeps
each container's existing shape, like `dprint-plugin-json` does.

## Formatting

`package.json` is matched by file _name_, and dprint gives a file to a
single plugin, so this plugin takes `package.json` away from
`dprint-plugin-json`. To avoid `package.json` becoming the one unformatted
file in a project, the sorted text is handed back to dprint's host formatter
under a virtual `.json` path — so `dprint-plugin-json`'s own configuration
applies, exactly as it would for any other JSON file.

If no JSON plugin is configured, the plugin formats the file itself using
the resolved `useTabs` / `indentWidth` / `newLineKind` settings, so the
result is the same either way.

Input is parsed with `jsonc-parser` — the same parser `dprint-plugin-json`
uses — so comments and trailing commas are accepted rather than rejected.
Comments travel with the property they belong to when sorting moves it, and
trailing commas are dropped on output, matching `dprint-plugin-json`'s
behavior for a `.json` file.

## Development

```sh
just            # list recipes
just test       # cargo test --all-features
just clippy     # strict lint
just ci         # fmt-check + clippy + test
just wasm       # build the wasm artifact (wasm-release profile)
(cd deployment/npm && bun install && bun run test)  # Bun smoke test
just diff PATH  # diff our output against `bunx sort-package-json`
```

The `sortpkg` binary is a feature-gated CLI for testing the sort logic in
isolation from dprint:

```sh
cargo run --features cli --bin sortpkg < some/package.json
cargo run --features cli --bin sortpkg -- --tabs some/package.json
cargo run --features cli --bin sortpkg -- --indent 4 some/package.json
```

Without `--tabs` or `--indent`, the file's existing indentation is kept —
the same thing the upstream `sort-package-json` CLI does.

## Releases

Tags are bare versions (no `v-` prefix), e.g. `0.1.0`. The release
workflow rejects `v0.1.0`-style tags. Each release publishes:

- `plugin.wasm` — the size-optimized plugin artifact
- `schema.json` — JSON Schema for the plugin's config block

## Credits

This is a port. All semantic credit goes to
[`keithamus/sort-package-json`](https://github.com/keithamus/sort-package-json),
the canonical implementation we mirror. Differences from upstream are
listed under [Parity](#parity).

## License

MIT — see [LICENSE].

[LICENSE]: ./LICENSE
[`schema.json`]: https://github.com/kjanat/dprint-plugin-sortpackagejson/releases/latest/download/schema.json
[`sort-package-json`]: https://github.com/keithamus/sort-package-json
[dprint]: https://dprint.dev
