-- Migration: M20260427_002_normalize_conn_status
-- Briefing 044g.1a-fix1
--
-- Normalise existing connections.conn_status values from the legacy
-- free-form vocabulary ('pending', 'pending_hello', 'connected') to the
-- canonical 044g.1a vocabulary ('New', 'Secured', 'Active'). New code
-- paths normalise on write via normalize_legacy_status; this migration
-- fixes rows that were written before the helper landed.
--
-- Idempotent: the WHERE clause filters only legacy values, so re-running
-- on already-canonical data is a no-op. The runner's gating on
-- sgx_migrations.name prevents double-application anyway.

BEGIN;

UPDATE connections
SET conn_status = CASE conn_status
    WHEN 'pending'       THEN 'New'
    WHEN 'pending_hello' THEN 'Secured'
    WHEN 'connected'     THEN 'Active'
    ELSE conn_status  -- already canonical, leave untouched
END,
    updated_at = unixepoch()
WHERE conn_status IN ('pending', 'pending_hello', 'connected');

INSERT INTO sgx_migrations (name, applied_at)
VALUES ('M20260427_002_normalize_conn_status', unixepoch());

COMMIT;
