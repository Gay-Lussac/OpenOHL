// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The network's schema: numbered forward migrations, each run once in a
//! transaction of its own and noted in `net_schema` (and `net_meta`
//! `schema`). A database of a newer schema is refused before anything in
//! it changes. (Refusing a migration without a recent backup is phase 13.)
//!
//! Columns as in the data model's "Server database (PostgreSQL)":
//! `docs/development/data-model.md`. Times are `timestamptz` (UTC); the
//! store gives them to the session as the project file writes them
//! (`YYYY-MM-DDTHH:MM:SS.sssZ`, [`TIME`]).

/// The newest schema this build knows.
pub const SCHEMA: i32 = 6;

/// A time column as the project file writes it.
pub const TIME: &str = "ohl_time";

/// The migrations, oldest first: (schema reached, what it does, SQL).
pub const MIGRATIONS: &[(i32, &str, &str)] = &[
    (
        1,
        "the network's tables (phase 4)",
        r#"
CREATE EXTENSION IF NOT EXISTS postgis;

CREATE FUNCTION ohl_time(t timestamptz) RETURNS text LANGUAGE sql IMMUTABLE STRICT AS
  $$ SELECT to_char(t AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') $$;

-- Network and users
CREATE TABLE net_meta (key text PRIMARY KEY, value text NOT NULL);
CREATE TABLE net_user (
  name text PRIMARY KEY, subject text UNIQUE, display text NOT NULL,
  active boolean NOT NULL DEFAULT true,
  created timestamptz NOT NULL DEFAULT clock_timestamp());
CREATE TABLE net_group (name text PRIMARY KEY, note text NOT NULL DEFAULT '');
CREATE TABLE net_member (
  grp text NOT NULL REFERENCES net_group ON DELETE CASCADE,
  member text NOT NULL REFERENCES net_user ON DELETE CASCADE,
  PRIMARY KEY (grp, member));

-- Rights and roles (ADR-0013): evaluated since phase 7 (schema 4).
CREATE TABLE role (name text PRIMARY KEY, shipped boolean NOT NULL, note text NOT NULL DEFAULT '');
CREATE TABLE role_permission (
  role text NOT NULL REFERENCES role ON DELETE CASCADE,
  permission text NOT NULL CHECK (permission IN ('see', 'edit', 'create', 'delete', 'review',
      'export', 'lock', 'request', 'manage_rights')),
  PRIMARY KEY (role, permission));
CREATE TABLE assignment (
  id bigserial PRIMARY KEY,
  subject_kind text NOT NULL CHECK (subject_kind IN ('user', 'group')),
  subject text NOT NULL,
  role text REFERENCES role,
  deny boolean NOT NULL DEFAULT false,
  scope_kind text NOT NULL CHECK (scope_kind IN ('network', 'region', 'line', 'route', 'kind',
      'object', 'library_kind', 'library_file', 'library_entry')),
  line text, route text, from_structure text, to_structure text,
  area geometry, kind text, object text,
  by_user text NOT NULL, time timestamptz NOT NULL DEFAULT clock_timestamp(),
  note text NOT NULL DEFAULT '',
  CHECK (role IS NOT NULL OR deny));
CREATE INDEX assignment_subject ON assignment (subject_kind, subject);
CREATE INDEX assignment_area ON assignment USING gist (area);

INSERT INTO role (name, shipped) VALUES ('viewer', true), ('designer', true), ('checker', true),
  ('library_maintainer', true), ('line_admin', true), ('network_admin', true);
INSERT INTO role_permission (role, permission) VALUES
  ('viewer', 'see'),
  ('designer', 'see'), ('designer', 'edit'), ('designer', 'create'), ('designer', 'delete'),
  ('designer', 'export'), ('designer', 'request'),
  ('checker', 'see'), ('checker', 'review'), ('checker', 'export'),
  ('library_maintainer', 'see'), ('library_maintainer', 'edit'), ('library_maintainer', 'create'),
  ('library_maintainer', 'delete'), ('library_maintainer', 'review'), ('library_maintainer', 'lock'),
  ('line_admin', 'see'), ('line_admin', 'edit'), ('line_admin', 'create'), ('line_admin', 'delete'),
  ('line_admin', 'review'), ('line_admin', 'export'), ('line_admin', 'lock'),
  ('line_admin', 'request'), ('line_admin', 'manage_rights'),
  ('network_admin', 'see'), ('network_admin', 'edit'), ('network_admin', 'create'),
  ('network_admin', 'delete'), ('network_admin', 'review'), ('network_admin', 'export'),
  ('network_admin', 'lock'), ('network_admin', 'request'), ('network_admin', 'manage_rights');

-- The models (design lines) and what the session keeps of each
CREATE TABLE line (
  id text PRIMARY KEY CHECK (id ~ '^[A-Za-z0-9][A-Za-z0-9_.-]{0,31}$'),
  name text NOT NULL,
  owner_group text REFERENCES net_group,
  built boolean NOT NULL DEFAULT false,
  library_pins jsonb NOT NULL DEFAULT '{}',
  source text NOT NULL DEFAULT '',
  created timestamptz NOT NULL DEFAULT clock_timestamp());
-- A model's own meta rows and users (the project file's ols_meta and
-- session_user).
CREATE TABLE line_meta (
  line_id text NOT NULL REFERENCES line ON DELETE CASCADE,
  key text NOT NULL, value text NOT NULL,
  PRIMARY KEY (line_id, key));
CREATE TABLE line_user (
  line_id text NOT NULL REFERENCES line ON DELETE CASCADE,
  name text NOT NULL, role text NOT NULL,
  PRIMARY KEY (line_id, name));
-- Objects: the session's JSON text as it wrote it (so a model exported is
-- the rows it was imported from); `json::jsonb` where a query needs its
-- fields. branch: the model's project line.
CREATE TABLE object (
  line_id text NOT NULL REFERENCES line ON DELETE CASCADE,
  branch text NOT NULL, kind text NOT NULL, id text NOT NULL,
  ord bigint NOT NULL, version bigint NOT NULL,
  json text NOT NULL, gone smallint NOT NULL DEFAULT 0,
  PRIMARY KEY (line_id, branch, kind, id));
-- Ids unique in the network: an element of the main branch of one model
-- cannot have the id of another model's (ADR-0013; clashes renamed at
-- import).
CREATE UNIQUE INDEX object_network_id ON object (kind, id)
  WHERE branch = 'main' AND gone = 0
    AND kind IN ('route', 'alignment', 'alignment_point', 'structure', 'section', 'jumper', 'spacer');
CREATE TABLE object_ref (
  line_id text NOT NULL REFERENCES line ON DELETE CASCADE,
  branch text NOT NULL, from_kind text NOT NULL, from_id text NOT NULL,
  to_kind text NOT NULL, to_id text NOT NULL, set_no integer NOT NULL DEFAULT 0,
  PRIMARY KEY (line_id, branch, from_kind, from_id, to_kind, to_id, set_no));
CREATE INDEX object_ref_to ON object_ref (branch, to_kind, to_id);
-- The log: seq numbered across the network (the one writer gives the
-- next), kept as it is when a model is copied (unique per model).
CREATE TABLE oplog (
  line_id text NOT NULL REFERENCES line ON DELETE CASCADE,
  seq bigint NOT NULL,
  time timestamptz NOT NULL DEFAULT clock_timestamp(),
  user_name text NOT NULL, kind text NOT NULL,
  command text NOT NULL, inverse text NOT NULL, objects text NOT NULL, sections text NOT NULL,
  undoes bigint, undone_by bigint, note text,
  branch text NOT NULL,
  PRIMARY KEY (line_id, seq));
CREATE INDEX oplog_seq ON oplog (seq);
CREATE INDEX oplog_user ON oplog (line_id, user_name, branch, seq);
-- The spatial index: the tile key (B-tree) and the bounds as a geometry
-- (GiST) beside it.
CREATE TABLE spatial (
  line_id text NOT NULL REFERENCES line ON DELETE CASCADE,
  branch text NOT NULL, kind text NOT NULL, id text NOT NULL,
  level smallint NOT NULL, tx bigint NOT NULL, ty bigint NOT NULL,
  x0 double precision NOT NULL, y0 double precision NOT NULL,
  x1 double precision NOT NULL, y1 double precision NOT NULL,
  geom geometry NOT NULL,
  PRIMARY KEY (line_id, branch, kind, id));
CREATE INDEX spatial_tile ON spatial (line_id, branch, level, tx, ty);
CREATE INDEX spatial_geom ON spatial USING gist (geom);
CREATE TABLE working_set (
  owner text NOT NULL, name text NOT NULL, routes jsonb NOT NULL,
  time timestamptz NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (owner, name));

-- Versions: blobs content-addressed for the whole network, each model's
-- use of them noted; revisions, branches and tags per model.
CREATE TABLE vc_blob (hash text PRIMARY KEY, data bytea NOT NULL);
CREATE TABLE vc_blob_use (
  line_id text NOT NULL REFERENCES line ON DELETE CASCADE,
  hash text NOT NULL REFERENCES vc_blob,
  PRIMARY KEY (line_id, hash));
CREATE TABLE vc_revision (
  line_id text NOT NULL REFERENCES line ON DELETE CASCADE,
  id text NOT NULL, n bigserial,
  parents text NOT NULL, author text NOT NULL,
  time timestamptz NOT NULL DEFAULT clock_timestamp(),
  message text NOT NULL, tree text NOT NULL, seq bigint NOT NULL,
  PRIMARY KEY (line_id, id));
CREATE TABLE vc_ref (
  line_id text NOT NULL REFERENCES line ON DELETE CASCADE,
  name text NOT NULL, kind text NOT NULL, revision text,
  protected boolean NOT NULL DEFAULT false, user_name text NOT NULL,
  time timestamptz NOT NULL DEFAULT clock_timestamp(),
  note text NOT NULL DEFAULT '', description text NOT NULL DEFAULT '', base text,
  PRIMARY KEY (line_id, name));
-- A model's edit grants as the desktop keeps them, until phase 7 makes
-- them assignments.
CREATE TABLE edit_grant (
  line_id text NOT NULL REFERENCES line ON DELETE CASCADE,
  id bigint NOT NULL, subject text NOT NULL, kinds text NOT NULL, objects text NOT NULL,
  area text, user_name text NOT NULL,
  time timestamptz NOT NULL DEFAULT clock_timestamp(),
  note text NOT NULL DEFAULT '',
  PRIMARY KEY (line_id, id));
CREATE TABLE vc_review (
  line_id text NOT NULL REFERENCES line ON DELETE CASCADE,
  id bigint NOT NULL, branch text NOT NULL, target text NOT NULL,
  revision text NOT NULL, author text NOT NULL,
  time timestamptz NOT NULL DEFAULT clock_timestamp(),
  note text NOT NULL, reviewers text NOT NULL, state text NOT NULL,
  library_kind text,
  PRIMARY KEY (line_id, id));
CREATE TABLE vc_review_note (
  seq bigserial PRIMARY KEY,
  line_id text NOT NULL REFERENCES line ON DELETE CASCADE,
  review bigint NOT NULL, user_name text NOT NULL,
  time timestamptz NOT NULL DEFAULT clock_timestamp(),
  verdict text NOT NULL, comment text NOT NULL);

-- Country libraries (phase 9)
CREATE TABLE lib_file (
  kind text NOT NULL, name text NOT NULL,
  locked boolean NOT NULL DEFAULT false, kind_locked boolean NOT NULL DEFAULT false,
  PRIMARY KEY (kind, name));
CREATE TABLE lib_entry (
  kind text NOT NULL, file text NOT NULL, entry text NOT NULL, version text NOT NULL,
  release text, withdrawn boolean NOT NULL DEFAULT false, withdrawn_note text,
  PRIMARY KEY (kind, file, entry, version));
CREATE TABLE lib_release (
  kind text NOT NULL, tag text NOT NULL, revision text NOT NULL, notes text NOT NULL,
  by_user text NOT NULL, time timestamptz NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (kind, tag));
CREATE TABLE lib_use (
  kind text NOT NULL, entry text NOT NULL, line text NOT NULL, route text NOT NULL DEFAULT '',
  branch text NOT NULL, object_kind text NOT NULL, object text NOT NULL,
  PRIMARY KEY (kind, entry, branch, object_kind, object));
CREATE TABLE lib_request (
  id bigserial PRIMARY KEY, kind text NOT NULL, file text NOT NULL, entry text NOT NULL,
  reason text NOT NULL, routes text[] NOT NULL DEFAULT '{}',
  state text NOT NULL CHECK (state IN ('draft', 'requested', 'granted', 'refused', 'withdrawn')),
  user_name text NOT NULL, maintainer text, note text,
  created timestamptz NOT NULL DEFAULT clock_timestamp(), decided timestamptz);

-- Terrain: a model's surfaces whole (as the project file keeps them);
-- tiles per grid tile later.
CREATE TABLE surface (
  line_id text NOT NULL REFERENCES line ON DELETE CASCADE,
  name text NOT NULL,
  time timestamptz NOT NULL DEFAULT clock_timestamp(),
  user_name text NOT NULL, source text NOT NULL,
  points bigint NOT NULL, triangles bigint NOT NULL,
  bounds text NOT NULL, data bytea NOT NULL,
  PRIMARY KEY (line_id, name));
CREATE TABLE surface_tile (
  name text NOT NULL, level integer NOT NULL, tx bigint NOT NULL, ty bigint NOT NULL,
  data bytea NOT NULL, PRIMARY KEY (name, level, tx, ty));

-- Staleness, notices, audit
CREATE TABLE stale (result_key text PRIMARY KEY, cause text NOT NULL,
  since timestamptz NOT NULL DEFAULT clock_timestamp());
CREATE TABLE notice (
  id bigserial PRIMARY KEY, user_name text NOT NULL,
  time timestamptz NOT NULL DEFAULT clock_timestamp(),
  text text NOT NULL, about jsonb NOT NULL, read boolean NOT NULL DEFAULT false);
CREATE TABLE audit (
  id bigserial PRIMARY KEY, time timestamptz NOT NULL DEFAULT clock_timestamp(),
  user_name text NOT NULL, action text NOT NULL, target text NOT NULL,
  before jsonb, after jsonb, detail text NOT NULL DEFAULT '');
-- Append only: no row of the audit changed or deleted, whoever asks.
CREATE FUNCTION audit_append_only() RETURNS trigger LANGUAGE plpgsql AS
  $$ BEGIN RAISE EXCEPTION 'the audit is append only'; END $$;
CREATE TRIGGER audit_append_only BEFORE UPDATE OR DELETE OR TRUNCATE ON audit
  FOR EACH STATEMENT EXECUTE FUNCTION audit_append_only();
"#,
    ),
    (
        2,
        "what the server keeps for its clients' interest (phase 5)",
        r#"
-- The routes each element is on (NULL: not counted yet, e.g. imported
-- before schema 2; the server counts them when it starts).
ALTER TABLE object ADD COLUMN routes text[];
CREATE INDEX object_routes ON object USING gin (routes);
-- The models and the package tiles an entry touched.
ALTER TABLE oplog ADD COLUMN lines text[] NOT NULL DEFAULT '{}';
ALTER TABLE oplog ADD COLUMN tiles bigint[] NOT NULL DEFAULT '{}';
CREATE INDEX oplog_tiles ON oplog USING gin (tiles);
-- Every change the server applied and broadcast, at its position
-- (seq, step), with the operation as the clients replay it.
CREATE TABLE stream (
  seq bigint NOT NULL, step integer NOT NULL CHECK (step >= 0),
  line_id text NOT NULL REFERENCES line ON DELETE CASCADE,
  branch text NOT NULL, user_name text NOT NULL,
  op text NOT NULL, first_seq bigint, log_seq bigint,
  tiles bigint[] NOT NULL DEFAULT '{}', routes text[] NOT NULL DEFAULT '{}',
  time timestamptz NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (seq, step));
CREATE INDEX stream_line ON stream (line_id, seq, step);
CREATE INDEX stream_tiles ON stream USING gin (tiles);
CREATE INDEX stream_routes ON stream USING gin (routes);
"#,
    ),
    (
        3,
        "saved working sets (phase 6)",
        r#"
-- A user's working set saved by name: its model and its picks (JSON as
-- text, as written); `routes` the routes its extent was on when saved.
ALTER TABLE working_set ADD COLUMN line_id text REFERENCES line ON DELETE CASCADE;
ALTER TABLE working_set ADD COLUMN picks text NOT NULL DEFAULT '[]';
"#,
    ),
    (
        4,
        "rights and roles evaluated (phase 7)",
        r#"
-- An assignment's scope in full: a library kind, file or entry; a region's
-- points as given (JSON as text, exact; `area` the same polygon for
-- PostGIS). A route or object scope has its line set when assigned.
ALTER TABLE assignment ADD COLUMN lib_kind text;
ALTER TABLE assignment ADD COLUMN lib_file text;
ALTER TABLE assignment ADD COLUMN lib_entry text;
ALTER TABLE assignment ADD COLUMN region text;
ALTER TABLE assignment ADD CONSTRAINT assignment_subject_named CHECK (subject <> '');
CREATE INDEX net_member_member ON net_member (member);
-- What a change touched, for the outgoing filter of a catch-up
-- (`kind/id`; NULL before schema 4: shown to those who see its whole
-- line only), and whether it touched its line at large.
ALTER TABLE stream ADD COLUMN objects text[];
ALTER TABLE stream ADD COLUMN whole boolean NOT NULL DEFAULT false;
-- The rights' revision: each change of users, groups, roles or
-- assignments adds one; the server reads the rights again when it moved.
INSERT INTO net_meta (key, value) VALUES ('rights_rev', '0') ON CONFLICT (key) DO NOTHING;
"#,
    ),
    (
        5,
        "country libraries (phase 9)",
        r#"
-- A library kind locked for the whole network (changed only by requests
-- granted and releases; maintainers are assignments, no list here).
CREATE TABLE lib_kind (
  kind text PRIMARY KEY,
  locked boolean NOT NULL DEFAULT false,
  by_user text NOT NULL, time timestamptz NOT NULL DEFAULT clock_timestamp(),
  note text NOT NULL DEFAULT '');
-- A file of the network's library as it is now (its head, released or
-- not); a file's own lock. (The draft's kind_locked: the kind's lock is
-- lib_kind's.)
ALTER TABLE lib_file DROP COLUMN kind_locked;
ALTER TABLE lib_file ADD COLUMN text text NOT NULL DEFAULT '';
ALTER TABLE lib_file ADD COLUMN hash text NOT NULL DEFAULT '';
ALTER TABLE lib_file ADD COLUMN by_user text NOT NULL DEFAULT '';
ALTER TABLE lib_file ADD COLUMN time timestamptz NOT NULL DEFAULT clock_timestamp();
-- Releases in the order made; each one's files by hash (texts in vc_blob,
-- each once for the network).
ALTER TABLE lib_release ADD COLUMN n bigserial;
CREATE UNIQUE INDEX lib_release_n ON lib_release (n);
CREATE TABLE lib_release_file (
  kind text NOT NULL, tag text NOT NULL, file text NOT NULL,
  hash text NOT NULL REFERENCES vc_blob,
  PRIMARY KEY (kind, tag, file),
  FOREIGN KEY (kind, tag) REFERENCES lib_release ON DELETE CASCADE);
-- Entry versions: each (kind, file, entry, version) with the release it
-- first came in. Withdrawn entries apart (an entry, every version).
ALTER TABLE lib_entry DROP COLUMN withdrawn;
ALTER TABLE lib_entry DROP COLUMN withdrawn_note;
CREATE TABLE lib_withdrawn (
  kind text NOT NULL, file text NOT NULL, entry text NOT NULL,
  note text NOT NULL DEFAULT '', by_user text NOT NULL,
  time timestamptz NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (kind, file, entry));
-- The use index: what each element of a model's main branch uses (a
-- wire, a structure type, an insulator assembly, a body's type; file
-- empty: named by id) and, for the kinds a line picks as a whole, each
-- entry of the files the line holds (object_kind 'library'). The routes
-- and places: the object's own (object.routes, spatial), joined.
DROP TABLE lib_use;
CREATE TABLE lib_use (
  line_id text NOT NULL REFERENCES line ON DELETE CASCADE,
  branch text NOT NULL, object_kind text NOT NULL, object text NOT NULL,
  kind text NOT NULL, file text NOT NULL DEFAULT '', entry text NOT NULL,
  PRIMARY KEY (line_id, branch, object_kind, object, kind, file, entry));
CREATE INDEX lib_use_entry ON lib_use (kind, entry);
-- A request: the entry asked for as a file of its own (its draft), the
-- model and branch where its requester uses it, the release it came in.
ALTER TABLE lib_request ADD COLUMN text text NOT NULL DEFAULT '';
ALTER TABLE lib_request ADD COLUMN line_id text REFERENCES line ON DELETE SET NULL;
ALTER TABLE lib_request ADD COLUMN branch text;
ALTER TABLE lib_request ADD COLUMN released text;
CREATE INDEX lib_request_user ON lib_request (user_name);
-- The libraries' revision: each change adds one; the server reads the
-- locks, requests and withdrawn entries again when it moved.
INSERT INTO net_meta (key, value) VALUES ('library_rev', '0') ON CONFLICT (key) DO NOTHING;
"#,
    ),
    (
        6,
        "offline working sets (phase 12)",
        r#"
-- An offline working set exported: by whom, of which model, from which
-- revision of its main line (base), the export's own revision (the
-- file's first: a bundle's base is checked against it) and its tree's
-- text (what the export holds, each object by the hash of its JSON — no
-- object again), the working set's objects (editable), its routes and
-- picks, the checks' reach, the bundles brought back (their heads and
-- branches: one is brought back once).
CREATE TABLE offline_export (
  id bigserial PRIMARY KEY,
  line_id text NOT NULL REFERENCES line ON DELETE CASCADE,
  user_name text NOT NULL,
  time timestamptz NOT NULL DEFAULT clock_timestamp(),
  base text NOT NULL,
  export text NOT NULL UNIQUE,
  tree text NOT NULL,
  wanted text NOT NULL,
  routes text NOT NULL,
  spec text NOT NULL,
  reach double precision NOT NULL,
  brought_back text NOT NULL DEFAULT '[]');
CREATE INDEX offline_export_user ON offline_export (user_name);
"#,
    ),
];
