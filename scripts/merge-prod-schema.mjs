#!/usr/bin/env node
// Merge a freshly generated ai-agent schema artifact into crates/tb-prod/schema.json.
//
// Why a merge and not a copy: ai-agent's generator moved custom actions into its own
// src/actions/ registry, so the artifact no longer carries three keys that tb-prod needs:
//
//   customActions         -> `tb-prod action` (all 36 actions)
//   searchFilterParam     -> `tb-prod search`
//   searchQuickResultType -> quick-search result types
//
// A plain copy destroys that behaviour. This script takes the new artifact as the base and
// carries those three keys forward from the vendored schema, per resource.
//
// Usage:
//   node scripts/merge-prod-schema.mjs --new /tmp/tb-prod-schema.new.json
//   node scripts/merge-prod-schema.mjs --new <path> --old <path> --out <path> [--dry-run]

import { readFileSync, writeFileSync } from 'node:fs';

const CARRIED_KEYS = ['customActions', 'searchFilterParam', 'searchQuickResultType'];

// Resources renamed upstream. Keys carried forward from `old` to a differently named
// resource in `new`. Add an entry only when the new resource is the same thing under a new
// type name and still supports the carried key.
//   slack_channel -> slack_channels (2026-08): type name aligned with the endpoint; the new
//   definition still exposes a `query` filter, so searchFilterParam stays valid.
const RENAMES = { slack_channel: 'slack_channels' };

function parseArgs(argv) {
  const out = { old: 'crates/tb-prod/schema.json', out: 'crates/tb-prod/schema.json', dryRun: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--new') out.new = argv[++i];
    else if (a === '--old') out.old = argv[++i];
    else if (a === '--out') out.out = argv[++i];
    else if (a === '--dry-run') out.dryRun = true;
    else throw new Error(`unknown argument: ${a}`);
  }
  if (!out.new) throw new Error('--new <path to generated artifact> is required');
  return out;
}

const stats = (s) => ({
  generatedAt: s.generatedAt,
  resources: Object.keys(s.resources).length,
  enums: Object.keys(s.enums).length,
  customActions: Object.values(s.resources).reduce((n, r) => n + Object.keys(r.customActions || {}).length, 0),
  fields: Object.values(s.resources).reduce((n, r) => n + Object.keys(r.fields || {}).length, 0),
});

const args = parseArgs(process.argv.slice(2));
const oldSchema = JSON.parse(readFileSync(args.old, 'utf8'));
const newSchema = JSON.parse(readFileSync(args.new, 'utf8'));

const before = stats(oldSchema);
const merged = structuredClone(newSchema);

const carriedTo = {};   // resource -> keys carried
const orphaned = [];    // { resource, key, value } that had nowhere to go

for (const [name, oldRes] of Object.entries(oldSchema.resources)) {
  const target = merged.resources[name] ? name : merged.resources[RENAMES[name]] ? RENAMES[name] : null;
  for (const key of CARRIED_KEYS) {
    const value = oldRes[key];
    if (value === undefined) continue;
    if (key === 'customActions' && Object.keys(value).length === 0) continue;
    if (!target) {
      orphaned.push({ resource: name, key, value });
      continue;
    }
    merged.resources[target][key] = value;
    (carriedTo[target] ??= []).push(key + (name === target ? '' : ` (from ${name})`));
  }
}

const after = stats(merged);

// --- report ---
const oldNames = new Set(Object.keys(oldSchema.resources));
const newNames = new Set(Object.keys(merged.resources));
const added = [...newNames].filter((n) => !oldNames.has(n)).sort();
const removed = [...oldNames].filter((n) => !newNames.has(n)).sort();
const lostFields = [];
for (const name of [...oldNames].filter((n) => newNames.has(n))) {
  const nf = new Set(Object.keys(merged.resources[name].fields || {}));
  const gone = Object.keys(oldSchema.resources[name].fields || {}).filter((f) => !nf.has(f));
  if (gone.length) lostFields.push([name, gone]);
}

const rows = [
  ['generatedAt', before.generatedAt, after.generatedAt],
  ['resources', before.resources, after.resources],
  ['enums', before.enums, after.enums],
  ['custom actions', before.customActions, after.customActions],
  ['total fields', before.fields, after.fields],
];
console.log('| metric | old | new |');
console.log('|---|---|---|');
for (const [k, a, b] of rows) console.log(`| ${k} | ${a} | ${b} |`);

console.log(`\nNew resource types (${added.length}) — no custom actions, none existed upstream:`);
console.log(added.length ? '  ' + added.join(', ') : '  (none)');
console.log(`\nRemoved resource types (${removed.length}):`);
console.log(removed.length ? '  ' + removed.join(', ') : '  (none)');
console.log(`\nResources that lost fields (${lostFields.length}):`);
for (const [name, gone] of lostFields) console.log(`  ${name}: ${gone.join(', ')}`);
if (!lostFields.length) console.log('  (none)');

console.log(`\nCarried forward into ${Object.keys(carriedTo).length} resources:`);
for (const [name, keys] of Object.entries(carriedTo).sort()) console.log(`  ${name}: ${keys.join(', ')}`);

console.log(`\nCould not be carried forward (${orphaned.length}):`);
for (const o of orphaned) console.log(`  ${o.resource}.${o.key} = ${JSON.stringify(o.value)}`);
if (!orphaned.length) console.log('  (none)');

if (before.customActions !== after.customActions) {
  console.error(`\nSTOP: custom action count changed ${before.customActions} -> ${after.customActions}. ` +
    `Do not force the merge — report which resources changed.`);
  process.exitCode = 1;
}

if (args.dryRun) {
  console.log('\n--dry-run: nothing written.');
} else {
  writeFileSync(args.out, JSON.stringify(merged, null, 2) + '\n');
  console.log(`\nWrote ${args.out}`);
}
