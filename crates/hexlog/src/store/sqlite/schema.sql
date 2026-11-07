CREATE TABLE tenants (
  tenant     TEXT PRIMARY KEY,
  anchor     TEXT NOT NULL,            -- sha256hex(anchor_doc)
  anchor_doc TEXT NOT NULL,            -- JCS do documento da âncora: formato, tenant e created_at
  created_at TEXT NOT NULL             -- igual a anchor_doc.created_at
) STRICT;

CREATE TABLE records (
  tenant       TEXT    NOT NULL REFERENCES tenants(tenant),
  seq          INTEGER NOT NULL CHECK (seq >= 1),
  id           TEXT    NOT NULL UNIQUE,                 -- "<tenant>:<uuidv7>"
  prev_hash    TEXT    NOT NULL,
  hash         TEXT    NOT NULL,
  body         TEXT    NOT NULL,                        -- JCS do record sem prev_hash: único insumo do hash
  -- colunas derivadas de body, conferidas na verificação da cadeia:
  at           TEXT    NOT NULL,
  type         TEXT    NOT NULL,                        -- tipo do node ou relation type
  lineage_kind TEXT    NOT NULL CHECK (lineage_kind IN ('node','relation')),
  lineage_key  TEXT    NOT NULL,                        -- id semântico, ou "relation_type|from|to"
  retired      INTEGER NOT NULL CHECK (retired IN (0,1)),
  revises      TEXT,                                    -- record id do latest record anterior
  relation_type TEXT,  from_id TEXT,  to_id TEXT,  from_origin TEXT,  to_origin TEXT,
  snapshot_kind TEXT,  snapshot_id TEXT,  external_id TEXT,  def_hash TEXT NOT NULL,
  PRIMARY KEY (tenant, seq)
) STRICT;

CREATE UNIQUE INDEX records_root      ON records (tenant, lineage_kind, lineage_key) WHERE revises IS NULL;
CREATE UNIQUE INDEX records_successor ON records (tenant, revises)                   WHERE revises IS NOT NULL;
CREATE INDEX records_lineage  ON records (tenant, lineage_kind, lineage_key, seq);
CREATE INDEX records_type     ON records (tenant, type, lineage_key);
CREATE INDEX records_snapshot    ON records (tenant, snapshot_kind, snapshot_id) WHERE snapshot_id IS NOT NULL;
CREATE INDEX records_relation_out ON records (tenant, relation_type, from_id) WHERE lineage_kind = 'relation';
CREATE INDEX records_relation_in  ON records (tenant, relation_type, to_id)   WHERE lineage_kind = 'relation';

CREATE TABLE batches (          -- índice de idempotência; só lotes com key
  tenant      TEXT NOT NULL REFERENCES tenants(tenant),
  key         TEXT NOT NULL,
  fingerprint TEXT NOT NULL,
  first_seq   INTEGER NOT NULL,
  item_count  INTEGER NOT NULL CHECK (item_count BETWEEN 1 AND 50),
  PRIMARY KEY (tenant, key),
  FOREIGN KEY (tenant, first_seq) REFERENCES records (tenant, seq)
) STRICT;

CREATE TRIGGER tenants_no_update BEFORE UPDATE ON tenants BEGIN SELECT RAISE(ABORT, 'append-only'); END;
CREATE TRIGGER tenants_no_delete BEFORE DELETE ON tenants BEGIN SELECT RAISE(ABORT, 'append-only'); END;
CREATE TRIGGER records_no_update BEFORE UPDATE ON records BEGIN SELECT RAISE(ABORT, 'append-only'); END;
CREATE TRIGGER records_no_delete BEFORE DELETE ON records BEGIN SELECT RAISE(ABORT, 'append-only'); END;
CREATE TRIGGER batches_no_update BEFORE UPDATE ON batches BEGIN SELECT RAISE(ABORT, 'append-only'); END;
CREATE TRIGGER batches_no_delete BEFORE DELETE ON batches BEGIN SELECT RAISE(ABORT, 'append-only'); END;

CREATE TRIGGER records_chain_guard BEFORE INSERT ON records
BEGIN
  SELECT RAISE(ABORT, 'seq out of order')
   WHERE NEW.seq <> COALESCE((SELECT MAX(seq) FROM records WHERE tenant = NEW.tenant), 0) + 1;
  SELECT RAISE(ABORT, 'prev_hash mismatch')
   WHERE NEW.prev_hash <> COALESCE(
         (SELECT hash   FROM records   WHERE tenant = NEW.tenant AND seq = NEW.seq - 1),
         (SELECT anchor FROM tenants WHERE tenant = NEW.tenant));
END;
