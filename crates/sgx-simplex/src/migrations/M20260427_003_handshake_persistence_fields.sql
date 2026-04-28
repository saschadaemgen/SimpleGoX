-- Migration: M20260427_003_handshake_persistence_fields
-- Briefing 044g.1b
--
-- Persist the four remaining RAM-only fields produced during the
-- Contact-Address handshake (rcv_id, rcv_dh_private, srv_dh_public,
-- peer_queue), plus add a to_subscribe flag to drive 044g.2's
-- boot-spawn loop. After this migration ships, every key the BG-loop
-- needs to resume a session is on disk; 044g.2 wires the entry point.

BEGIN;

-- The three small fixed-size fields: separate columns
ALTER TABLE connections ADD COLUMN rcv_id          BLOB;
ALTER TABLE connections ADD COLUMN rcv_dh_private  BLOB;
ALTER TABLE connections ADD COLUMN srv_dh_public   BLOB;

-- The peer reply queue: postcard-encoded blob analogous to BobRatchet (044d).
-- format_version distinguishes future encoding generations so SmpQueueInfo
-- field additions become migration-free in SQL.
ALTER TABLE connections ADD COLUMN peer_queue_blob           BLOB;
ALTER TABLE connections ADD COLUMN peer_queue_format_version INTEGER;

-- Boot-spawn flag - default 1 for new rows so every fresh handshake is
-- picked up by 044g.2's resubscribe loop. Legacy rows from 044g.1a's
-- backfill (NULL rcv_id) are explicitly cleared below for honesty:
-- they cannot be re-subscribed without the missing keys, and the flag's
-- value should match actual capability.
ALTER TABLE connections ADD COLUMN to_subscribe INTEGER NOT NULL DEFAULT 1;

UPDATE connections
SET to_subscribe = 0
WHERE rcv_id IS NULL;

INSERT INTO sgx_migrations (name, applied_at)
VALUES ('M20260427_003_handshake_persistence_fields', unixepoch());

COMMIT;
