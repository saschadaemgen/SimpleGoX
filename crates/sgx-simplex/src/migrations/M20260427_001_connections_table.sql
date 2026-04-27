-- Migration: M20260427_001_connections_table
-- Briefing 044g.1a — connections-Table Schema-Refactor
--
-- Move queue-specific state out of `contacts` into a new `connections` table.
-- Backfill from existing data (1:1 legacy mapping). Update ratchet_states FK.
-- Drop migrated and dead columns from contacts.
--
-- Behavior-equivalent for the Contact-Address flow: callers in service.rs
-- continue to use contact_id-keyed APIs; queue_store.rs internally resolves
-- to connection_id via resolve_active_connection_id (Tier-2 wrapper pattern).

BEGIN;
PRAGMA defer_foreign_keys = ON;

-- ============================================================================
-- Step 1: connections table + indexes
-- ============================================================================

CREATE TABLE connections (
    connection_id      INTEGER PRIMARY KEY AUTOINCREMENT,
    contact_id         TEXT NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,

    -- Stable per-connection cross-reference. Legacy 1:1 backfill puts the
    -- contact_id bytes here; 044g.2+ generates a fresh agent_conn_id (e.g.
    -- UUID v4) for each new connection a contact gets.
    agent_conn_id      BLOB NOT NULL UNIQUE,

    -- QueueStatus enum: 'New' | 'Confirmed' | 'Secured' | 'Active' | 'Disabled'.
    -- Maps the former contacts.status TEXT (which was free-form: 'pending',
    -- 'connected', 'pending_hello'). Backfill chooses 'Active' iff
    -- queue_auth_private IS NOT NULL (handshake reached NEW success).
    conn_status        TEXT NOT NULL DEFAULT 'New',

    -- Type discriminator for future expansion: 'contact' | 'member' | 'snd_file'.
    conn_type          TEXT NOT NULL DEFAULT 'contact',

    -- SMP server endpoint (was contacts.{server_host, server_port, fingerprint}).
    server_host        TEXT NOT NULL,
    server_port        INTEGER NOT NULL DEFAULT 5223,
    fingerprint        TEXT NOT NULL,

    -- Recipient queue identifiers (was contacts.{queue_id, sender_key}).
    queue_id           TEXT,
    sender_key         TEXT,

    -- Recipient queue auth (Briefings 044c, 044e).
    queue_auth_private BLOB,
    rcv_auth_private   BLOB,

    -- E2E handshake material (was on contacts).
    e2e_key1_private   BLOB,
    e2e_key1_public    BLOB,
    e2e_key2_private   BLOB,
    e2e_key2_public    BLOB,
    peer_snd_id        BLOB,
    peer_dh_public     BLOB,
    peer_e2e_pub       BLOB,
    sender_auth_key_private BLOB,
    sender_auth_key_public  BLOB,
    own_l2_ephemeral_private BLOB,

    -- HELLO idempotency (Briefing 041b-hello).
    hello_sent         INTEGER NOT NULL DEFAULT 0,

    -- Timestamps.
    created_at         INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at         INTEGER NOT NULL DEFAULT (unixepoch()),

    -- Closest analog to simplexmq's `PRIMARY KEY(host, port, rcv_id)`. Uses
    -- queue_id for now; 044g.1b will extend with rcv_id when that column
    -- becomes first-class.
    UNIQUE(server_host, server_port, queue_id)
);

CREATE INDEX idx_connections_contact_id ON connections(contact_id);
CREATE INDEX idx_connections_agent_conn_id ON connections(agent_conn_id);
CREATE INDEX idx_connections_status ON connections(contact_id, conn_status);

-- ============================================================================
-- Step 2: Backfill from existing contacts (1:1 legacy mapping)
-- ============================================================================

INSERT INTO connections (
    contact_id, agent_conn_id, conn_status, conn_type,
    server_host, server_port, fingerprint, queue_id, sender_key,
    queue_auth_private, rcv_auth_private,
    e2e_key1_private, e2e_key1_public, e2e_key2_private, e2e_key2_public,
    peer_snd_id, peer_dh_public, peer_e2e_pub,
    sender_auth_key_private, sender_auth_key_public, own_l2_ephemeral_private,
    hello_sent, created_at, updated_at
)
SELECT
    id,
    CAST(id AS BLOB),
    CASE WHEN queue_auth_private IS NOT NULL THEN 'Active' ELSE 'New' END,
    'contact',
    COALESCE(server_host, 'unknown'),
    COALESCE(server_port, 5223),
    COALESCE(fingerprint, ''),
    queue_id,
    sender_key,
    queue_auth_private,
    rcv_auth_private,
    e2e_key1_private, e2e_key1_public, e2e_key2_private, e2e_key2_public,
    peer_snd_id, peer_dh_public, peer_e2e_pub,
    sender_auth_key_private, sender_auth_key_public, own_l2_ephemeral_private,
    COALESCE(hello_sent, 0),
    COALESCE(created_at, unixepoch()),
    unixepoch()
FROM contacts;

-- ============================================================================
-- Step 3: ratchet_states FK swap (clean break: drop legacy contact_id column)
-- ============================================================================

CREATE TABLE ratchet_states_new (
    connection_id  INTEGER PRIMARY KEY REFERENCES connections(connection_id) ON DELETE CASCADE,
    state_blob     BLOB NOT NULL,
    format_version INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL DEFAULT (unixepoch())
);

INSERT INTO ratchet_states_new (connection_id, state_blob, format_version, updated_at)
SELECT c.connection_id, r.state_blob, r.format_version, r.updated_at
FROM ratchet_states r
JOIN connections c ON c.contact_id = r.contact_id;

DROP TABLE ratchet_states;
ALTER TABLE ratchet_states_new RENAME TO ratchet_states;

-- ============================================================================
-- Step 4: Drop migrated and dead columns from contacts
-- ============================================================================
-- Migrated: server_host, server_port, fingerprint, queue_id, sender_key, status,
--           rcv_id, rcv_dh_private, rcv_dh_public, rcv_auth_private,
--           e2e_key{1,2}_{private,public}, peer_snd_id, peer_dh_public, peer_e2e_pub,
--           sender_auth_key_{private,public}, own_l2_ephemeral_private, hello_sent,
--           queue_auth_private (22 columns)
-- Dead per Briefing 044d / W0 inventory: snd_id, snd_auth_private, msg_id_send,
--           prev_msg_hash (4 columns)

ALTER TABLE contacts DROP COLUMN server_host;
ALTER TABLE contacts DROP COLUMN server_port;
ALTER TABLE contacts DROP COLUMN fingerprint;
ALTER TABLE contacts DROP COLUMN queue_id;
ALTER TABLE contacts DROP COLUMN sender_key;
ALTER TABLE contacts DROP COLUMN status;
ALTER TABLE contacts DROP COLUMN rcv_id;
ALTER TABLE contacts DROP COLUMN snd_id;
ALTER TABLE contacts DROP COLUMN rcv_auth_private;
ALTER TABLE contacts DROP COLUMN rcv_dh_private;
ALTER TABLE contacts DROP COLUMN rcv_dh_public;
ALTER TABLE contacts DROP COLUMN snd_auth_private;
ALTER TABLE contacts DROP COLUMN e2e_key1_private;
ALTER TABLE contacts DROP COLUMN e2e_key1_public;
ALTER TABLE contacts DROP COLUMN e2e_key2_private;
ALTER TABLE contacts DROP COLUMN e2e_key2_public;
ALTER TABLE contacts DROP COLUMN peer_snd_id;
ALTER TABLE contacts DROP COLUMN peer_dh_public;
ALTER TABLE contacts DROP COLUMN msg_id_send;
ALTER TABLE contacts DROP COLUMN prev_msg_hash;
ALTER TABLE contacts DROP COLUMN sender_auth_key_private;
ALTER TABLE contacts DROP COLUMN sender_auth_key_public;
ALTER TABLE contacts DROP COLUMN peer_e2e_pub;
ALTER TABLE contacts DROP COLUMN own_l2_ephemeral_private;
ALTER TABLE contacts DROP COLUMN hello_sent;
ALTER TABLE contacts DROP COLUMN queue_auth_private;

-- ============================================================================
-- Step 5: Register migration as applied
-- ============================================================================

INSERT INTO sgx_migrations (name, applied_at)
VALUES ('M20260427_001_connections_table', unixepoch());

COMMIT;
PRAGMA defer_foreign_keys = OFF;
