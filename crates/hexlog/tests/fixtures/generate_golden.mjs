// Gera as fixtures douradas da T2 com o Hexlog TS (commit 5d1e22b): canonicalize do npm + sha256 hex,
// a mesma conta de src/domain/chain.ts:64-67 (hashLink) e :55-57 (hashOfJcs).
import { createRequire } from 'node:module';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
// Uso, da raiz do workspace: node crates/hexlog/tests/fixtures/generate_golden.mjs
// Requer o repositório do Hexlog TS (commit 5d1e22b) com node_modules instalado; HEXLOG_TS_DIR muda o caminho.
// Gera golden_records.json, golden_bodies.json e golden_fingerprints.json. rfc8785_vectors.json é fixture estática
// (RFC 8785 e cyberphone/json-canonicalization) e este script não a toca.
const HEXLOG_TS = process.env.HEXLOG_TS_DIR ?? '/home/gabriel/personal/hexlog';
const require = createRequire(`${HEXLOG_TS}/`);
const canonicalize = require('canonicalize').default ?? require('canonicalize');
const canonVersion = JSON.parse(readFileSync(`${HEXLOG_TS}/node_modules/canonicalize/package.json`, 'utf8')).version;
const sha = (s) => createHash('sha256').update(s).digest('hex');
const OUT = dirname(fileURLToPath(import.meta.url));
const provenance = (extra) => ({
  generated_with: 'node crates/hexlog/tests/fixtures/generate_golden.mjs (from the workspace root)',
  hexlog_ts_commit: '5d1e22b',
  hexlog_ts_repo: 'hexlog TS repository, read only, node_modules as installed (HEXLOG_TS_DIR, default ~/personal/hexlog)',
  canonicalize_npm: canonVersion,
  ...extra,
});
const write = (name, obj) => writeFileSync(`${OUT}/${name}`, JSON.stringify(obj, null, 1) + '\n');

// ---- golden_records.json
const ZERO = '0'.repeat(64);
const R = (n) => `acme:0192a000-0000-7000-8000-${String(n).padStart(12, '0')}`;
const docs = {
  simple: { v: 1, tenant: 'acme', seq: 1, id: R(1), type: 'note', retirement: false, data: { title: 'hello', count: 3, ok: true, none: null } },
  unicode: { v: 1, tenant: 'acme', seq: 2, id: R(2), type: 'note', retirement: false, data: { 'é': 'café ☕', '😀': 'emoji', '€': 'euro', 'a/b': 'slash', quote: 'q"\\', ctl: '\u0001\n\t' } },
  limit_integers: { v: 1, tenant: 'acme', seq: 3, id: R(3), type: 'note', retirement: false, data: { max: 9007199254740991, min: -9007199254740991, zero: 0 } },
};
let prev = ZERO;
const records = {};
for (const [k, body] of Object.entries(docs)) {
  const jcs = canonicalize(body);
  const hash = sha(prev + jcs);
  records[k] = { prev_hash: prev, document: body, jcs, hash };
  prev = hash;
}
const tenant = 'acme', created_at = '2026-11-07T12:00:00.000Z';
const anchorDoc = canonicalize({ created_at, format: 'hexlog-chain/1', tenant });
write('golden_records.json', {
  ...provenance({ formula: 'hash = sha256hex(prev_hash + canonicalize(document)); anchor = sha256hex(canonicalize({created_at, format:"hexlog-chain/1", tenant}))' }),
  anchor: { tenant, created_at, anchor_doc: anchorDoc, anchor: sha(anchorDoc) },
  records,
});

// ---- golden_bodies.json
const DH = 'a'.repeat(64), FP = 'b'.repeat(64), SNAP = 'c'.repeat(40), AT = '2026-11-07T12:00:00.000Z';
const author = { agent: 'ag', model: 'm', client: 'cli' };
const common = (seq) => ({ v: 1, tenant: 'acme', seq, id: R(seq), at: AT, author, def_hash: DH });
const relLineage = { kind: 'relation', relation_type: 'after', from: 'a.one', to: 'n.other', from_origin: 'tenant', to_origin: 'native' };
const bodies = {
  node_full: { ...common(3), type: 'note', lineage: { kind: 'node', id: 'area.sub.item' }, revises: R(1), retirement: false,
    data: { k: 'v', n: 7, 'a/b': 'é\u0001"\\' }, snapshot: { kind: 'git-commit', id: SNAP }, external_id: 'x', batch: { fingerprint: FP, key: 'K1', count: 2 } },
  relation_native: { ...common(4), type: 'after', lineage: relLineage, native_catalog: 'v1', retirement: false },
  retirement_node: { ...common(5), type: 'note', lineage: { kind: 'node', id: 'a.two' }, revises: R(2), retirement: true },
  retirement_relation: { ...common(6), type: 'after', lineage: relLineage, native_catalog: 'v1', revises: R(4), retirement: true },
  node_keyless_batch_first: { ...common(1), type: 'note', lineage: { kind: 'node', id: 'a.one' }, retirement: false, data: {}, batch: { fingerprint: FP, count: 1 } },
};
const gb = {};
for (const [k, b] of Object.entries(bodies)) { const jcs = canonicalize(b); gb[k] = { fields: b, jcs, hash: sha(ZERO + jcs) }; }
write('golden_bodies.json', { ...provenance({ formula: 'hash = sha256hex(prev_hash + jcs), prev_hash = 64 zeros; fields = the body object, field by field', prev_hash: ZERO }), shapes: gb });

// ---- golden_fingerprints.json
const B = R(1), SN = { kind: 'git-commit', id: SNAP };
const items = {
  node_full: { body: { kind: 'node', id: 'a.b', type: 'note', data: { k: 'v' } }, base: B, snapshot: SN, external_id: 'x' },
  node_minimal: { body: { kind: 'node', id: 'a.b', type: 'note', data: {} } },
  relation: { body: { kind: 'relation', relation_type: 'after', from: 'a.b', to: 'c.d' } },
  retirement_node: { body: { kind: 'retirement', lineage: { kind: 'node', id: 'a.b' } }, base: B },
  retirement_relation: { body: { kind: 'retirement', lineage: { kind: 'relation', relation_type: 'after', from: 'a.b', to: 'c.d' } }, base: B },
};
const fp = {};
for (const [k, it] of Object.entries(items)) { const jcs = canonicalize([it]); fp[k] = { items: [it], jcs, fingerprint: sha(jcs) }; }
const order = (name, list) => { const jcs = canonicalize(list); fp[name] = { items: list, jcs, fingerprint: sha(jcs) }; };
order('list_node_then_relation', [items.node_full, items.relation]);
order('list_relation_then_node', [items.relation, items.node_full]);
write('golden_fingerprints.json', { ...provenance({ formula: 'fingerprint = sha256hex(canonicalize(array of items)); item JSON shape from seams contract, costura 5 (absent optional is omitted, never null)' }), shapes: fp });
console.log('ok');
