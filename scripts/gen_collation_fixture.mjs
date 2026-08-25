// Dev-time generator for tests/fixtures/collation.txt — the ground truth for
// src/sort/collate.rs. Run with: node scripts/gen_collation_fixture.mjs
//
// Emits `a<TAB>b<TAB>expected` where expected is -1, 0 or 1, taken from
// String.prototype.localeCompare(_, 'en') — the comparator upstream
// sort-package-json uses for npm-style dependency ordering.
import { writeFileSync } from "node:fs";

const ALPHABET = [..."abzAZ019-_.@/~$"];
const REAL_NAMES = [
	"react",
	"React",
	"react-dom",
	"rabbit",
	"@types/node",
	"@types/react",
	"@babel/core",
	"a-b",
	"a_b",
	"a1b",
	"a.b",
	"lodash",
	"lodash.merge",
	"eslint",
	"eslint-config-prettier",
	"eslint_legacy",
	"Zone.js",
	"zone.js",
	"@scope/x",
	"zzz",
	"foo2",
	"foo10",
	"_private",
	"AWS-sdk",
	"aws-sdk",
];

// Deterministic PRNG so the fixture is reproducible. Math.imul keeps the
// multiply in 32-bit integer space: seed * 1103515245 overflows the 53-bit
// float mantissa, and the rounding that follows collapses the state space to
// roughly half its period, which quietly narrows the corpus.
let seed = 0x2f6e2b1;
const rnd = () => ((seed = (Math.imul(seed, 1103515245) + 12345) & 0x7fffffff) / 0x80000000);
const word = () => {
	let s = "";
	const len = 1 + Math.floor(rnd() * 6);
	for (let i = 0; i < len; i++) s += ALPHABET[Math.floor(rnd() * ALPHABET.length)];
	return s;
};

const pairs = [];
for (const a of REAL_NAMES) for (const b of REAL_NAMES) pairs.push([a, b]);
for (let i = 0; i < 4000; i++) pairs.push([word(), word()]);

const lines = pairs.map(([a, b]) => `${a}\t${b}\t${Math.sign(a.localeCompare(b, "en"))}`);
writeFileSync(new URL("../tests/fixtures/collation.txt", import.meta.url), lines.join("\n") + "\n");
console.log(`wrote ${lines.length} pairs`);
