# dprint-plugin-sortpackagejson

Rust port of [`sort-package-json`](https://github.com/keithamus/sort-package-json) as a [dprint](https://dprint.dev) plugin.

Sorts a `package.json` into the canonical key order used across the npm
ecosystem: lifecycle scripts grouped pre/main/post, dependencies
alphabetized, exports/conditions ordered correctly, and the rest.

## Install

In your `dprint.json`:

```json
{
	"plugins": [
		"https://github.com/kjanat/dprint-plugin-sortpackagejson/releases/download/0.1.0/plugin.wasm"
	],
	"sortPackageJson": {}
}
```

The plugin only formats files literally named `package.json`; everything
else stays with `dprint-plugin-json`.

## Configuration

| Key                | Type                           | Default          | Description                                                                                         |
| ------------------ | ------------------------------ | ---------------- | --------------------------------------------------------------------------------------------------- |
| `sortOrder`        | `string[]`                     | `[]`             | Custom top-level key order; empty falls back to the canonical [`sort-package-json`](#parity) order. |
| `sortDependencies` | `boolean`                      | `true`           | Alphabetize entries inside dependency objects.                                                      |
| `sortScripts`      | `boolean`                      | `true`           | Apply pre/main/post grouping + colon-namespace handling + npm-run-all chain detection.              |
| `sortNested`       | `boolean`                      | `true`           | Apply nested-section sort rules (engines, exports, eslintConfig, prettier, workspaces, pnpm, ...).  |
| `unknownKeys`      | `"alphabetical" \| "preserve"` | `"alphabetical"` | How to order top-level keys not present in the canonical list.                                      |

The IDE-autocomplete schema for these options lives at
[`schema.json`](https://github.com/kjanat/dprint-plugin-sortpackagejson/releases/download/0.1.0/schema.json)
and is regenerated from the Rust `Configuration` struct via `schemars`;
drift is enforced by `tests/schema_in_sync.rs`.

## Parity

Behavioral target: `sort-package-json` v3.6.1. Drifts that you should know
about, all listed in module-level docs:

- **Dependency comparator**: upstream switches between locale-aware
  (`localeCompare(_, 'en')`) and plain string compare based on detected
  package manager (npm vs yarn/pnpm). For all-ASCII lowercase keys (the
  realistic majority) the two orderings agree, so we ship plain compare in
  0.1.0.
- **`pnpm.overrides` semver compare**: upstream uses `semver` to break ties
  for same-package-different-range keys; we fall back to plain
  lexicographic on the range portion to keep the wasm artifact small.
- **`imports`**: upstream does not reorder imports; we don't either.

## Development

```sh
just            # list recipes
just test       # cargo test --all-features
just clippy     # strict lint
just ci         # fmt-check + clippy + test
just wasm-opt   # build + size-optimize the wasm artifact
just diff PATH  # diff our output against `bunx sort-package-json`
```

The `sortpkg` binary is a feature-gated CLI for testing the sort logic in
isolation from dprint:

```sh
cargo run --features cli --bin sortpkg < some/package.json
```

## Releases

Tags are bare versions (no `v-` prefix), e.g. `0.1.0`. The release
workflow rejects `v0.1.0`-style tags. Each release publishes:

- `plugin.wasm` — the size-optimized plugin artifact
- `schema.json` — JSON Schema for the plugin's config block
- `*.sha256` — checksums for both

## Credits

This is a port. All semantic credit goes to
[`keithamus/sort-package-json`](https://github.com/keithamus/sort-package-json),
the canonical implementation we mirror. Differences from upstream are
listed under [Parity](#parity).

## License

MIT — see [LICENSE](./LICENSE).
