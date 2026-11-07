// Gera def_hashes.json com o Hexlog TS (commit 5d1e22b): canonicalize do npm + sha256 hex.
// Uso, da raiz do workspace: node crates/hexlog/tests/fixtures/generate_def_hashes.mjs
// Requer o repositório do Hexlog TS (commit 5d1e22b) com node_modules instalado; HEXLOG_TS_DIR muda o caminho.
// Os seis documentos e os hashes esperados vêm da tabela do item (h) da D-7 do log do crítico; o script
// recusa gravar se algum hash calculado divergir da tabela.
import { createRequire } from 'node:module';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const HEXLOG_TS = process.env.HEXLOG_TS_DIR ?? '/home/gabriel/personal/hexlog';
const require = createRequire(`${HEXLOG_TS}/`);
const canonicalize = require('canonicalize').default ?? require('canonicalize');
const canonVersion = JSON.parse(readFileSync(`${HEXLOG_TS}/node_modules/canonicalize/package.json`, 'utf8')).version;
const sha = (s) => createHash('sha256').update(s).digest('hex');
const OUT = dirname(fileURLToPath(import.meta.url));

// kind, documento como escrito (entrada do Catalog) e hash da tabela da D-7.
const cases = [
  { kind: 'type', document: { name: 'note', schema: { type: 'object' }, requires_snapshot: false }, table: 'dd42860a5d64e12ee33e608d711b6efaa53bdbf1d79eaa531b3587f82b754447' },
  { kind: 'type', document: { schema: { type: 'object' }, name: 'note' }, table: 'dd42860a5d64e12ee33e608d711b6efaa53bdbf1d79eaa531b3587f82b754447' },
  { kind: 'type', document: { name: 'note', schema: { type: 'object', properties: { 'é': { type: 'string' }, a: { type: 'integer', minimum: -5 } } }, requires_snapshot: true, equivalent: 'https://example.com/n' }, table: '817730a409826e94700ef25d541514a48aa6d486ece026be55c058713372ac75' },
  { kind: 'relation', document: { name: 'after', from: ['note', 'task'], to: ['note'], acyclic: true }, table: 'ff31663197ac307219e96d5cfcbdf9e7f70479248fb3e14dae404af5d2e404d5' },
  { kind: 'relation', document: { name: 'after', from: ['task', 'note'], to: ['note'], acyclic: true }, table: 'fb3fa70df5f1b3060e97550892bf49498041c65bb823613a27e74498cabf5613' },
  { kind: 'relation', document: { name: 'links' }, table: '16f4fa81d70a0010f92708972fcb19aa4b928b6772dbce53d61419810e2c1197' },
];

// Forma serializada do contrato: requires_snapshot e acyclic sempre escritos (padrão false);
// equivalent, from e to omitidos quando ausentes, nunca null.
const normalize = ({ kind, document: d }) => kind === 'type'
  ? { name: d.name, schema: d.schema, requires_snapshot: d.requires_snapshot ?? false, ...(d.equivalent !== undefined ? { equivalent: d.equivalent } : {}) }
  : { name: d.name, ...(d.from ? { from: d.from } : {}), ...(d.to ? { to: d.to } : {}), acyclic: d.acyclic ?? false, ...(d.equivalent !== undefined ? { equivalent: d.equivalent } : {}) };

const definitions = cases.map((c, i) => {
  const normalized = normalize(c);
  const def_hash = sha(canonicalize(normalized));
  if (def_hash !== c.table) {
    console.error(`DIVERGE case ${i}: computed ${def_hash}, D-7 table ${c.table}`);
    process.exit(1);
  }
  return { kind: c.kind, document: c.document, def_hash };
});

writeFileSync(`${OUT}/def_hashes.json`, JSON.stringify({
  generated_with: 'node crates/hexlog/tests/fixtures/generate_def_hashes.mjs (from the workspace root)',
  hexlog_ts_commit: '5d1e22b',
  hexlog_ts_repo: 'hexlog TS repository, read only, node_modules as installed (HEXLOG_TS_DIR, default ~/personal/hexlog)',
  canonicalize_npm: canonVersion,
  formula: 'def_hash = sha256hex(canonicalize(normalized definition)); normalized: requires_snapshot and acyclic always present (default false), equivalent/from/to omitted when absent',
  definitions,
}, null, 1) + '\n');
console.log(`ok: ${definitions.length} definitions match the D-7 table`);
