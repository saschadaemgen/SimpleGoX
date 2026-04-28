//! SQLite persistence for SimpleX contacts, queues, and messages.

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

const SCHEMA: &str = r#"
    CREATE TABLE IF NOT EXISTS profile (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS contacts (
        id           TEXT PRIMARY KEY,
        display_name TEXT,
        server_host  TEXT,
        server_port  INTEGER DEFAULT 5223,
        fingerprint  TEXT,
        queue_id     TEXT,
        sender_key   TEXT,
        status       TEXT NOT NULL DEFAULT 'pending',
        created_at   INTEGER NOT NULL DEFAULT (unixepoch()),
        -- Handshake state (saved during AgentInvitation build)
        rcv_id              BLOB,
        snd_id              BLOB,
        rcv_auth_private    BLOB,
        rcv_dh_private      BLOB,
        rcv_dh_public       BLOB,
        snd_auth_private    BLOB,
        e2e_key1_private    BLOB,
        e2e_key1_public     BLOB,
        e2e_key2_private    BLOB,
        e2e_key2_public     BLOB,
        peer_snd_id         BLOB,
        peer_dh_public      BLOB,
        msg_id_send         INTEGER DEFAULT 0,
        prev_msg_hash       BLOB
    );

    CREATE TABLE IF NOT EXISTS messages (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        contact_id TEXT NOT NULL REFERENCES contacts(id),
        body       TEXT NOT NULL,
        direction  TEXT NOT NULL,
        created_at INTEGER NOT NULL DEFAULT (unixepoch())
    );

    -- Briefing 044d: blob-versioned ratchet state. Replaces the original
    -- 16-column generic Double Ratchet schema (modeled on simplexmq's
    -- Haskell Ratchet) which never received a single INSERT in the Rust
    -- codebase and did not match BobRatchet's actual field set. The
    -- state_blob holds a postcard-encoded `PersistedRatchetV` enum;
    -- format_version distinguishes generations so future BobRatchet
    -- field additions become migration-free in SQL. ON DELETE CASCADE
    -- keeps the row aligned with contact lifecycle (matches the
    -- existing WipeAllSimplexContacts semantics).
    CREATE TABLE IF NOT EXISTS ratchet_states (
        contact_id      TEXT PRIMARY KEY REFERENCES contacts(id) ON DELETE CASCADE,
        state_blob      BLOB NOT NULL,
        format_version  INTEGER NOT NULL,
        updated_at      INTEGER NOT NULL DEFAULT (unixepoch())
    );

    CREATE TABLE IF NOT EXISTS sender_auth (
        contact_id  TEXT PRIMARY KEY,
        private_key BLOB NOT NULL,
        public_key  BLOB NOT NULL
    );

    -- Singleton user profile. CHECK forces a single row (id = 1) so there
    -- is exactly one local user profile. The legacy key-value `profile`
    -- table above is retained for settings like `proxy` and backwards
    -- compatibility with `save_profile(name) / get_profile_name()`.
    CREATE TABLE IF NOT EXISTS user_profile (
        id               INTEGER PRIMARY KEY CHECK (id = 1),
        display_name     TEXT NOT NULL DEFAULT '',
        full_name        TEXT NOT NULL DEFAULT '',
        bio              TEXT NOT NULL DEFAULT '',
        preferences_json TEXT NOT NULL DEFAULT '{}',
        created_at       INTEGER NOT NULL DEFAULT (unixepoch()),
        updated_at       INTEGER NOT NULL DEFAULT (unixepoch())
    );
"#;

/// Wraps a SQLite connection for SimpleX data persistence.
pub struct QueueStore {
    conn: Mutex<Connection>,
}

impl QueueStore {
    /// Open or create the SQLite database and run migrations.
    pub fn open(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir).ok();
        let db_path = data_dir.join("simplex.db");
        let mut conn = Connection::open(&db_path)?;

        // Briefing 044d: WAL journal mode for crash resistance. With
        // ratchet state now persisted on every send / recv, a process
        // crash between the SQLite write and a clean shutdown is more
        // likely to land mid-write than the pre-044d workload. WAL
        // makes recovery cheap and avoids the rollback-journal corner
        // cases on Windows file-locking. Idempotent - re-issuing the
        // pragma on a WAL DB is a no-op.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        // FK enforcement is required for the ON DELETE CASCADE on
        // ratchet_states(contact_id) -> contacts(id) added in 044d
        // and connections.contact_id -> contacts(id) added in 044g.1a.
        // SQLite leaves foreign_keys OFF by default per connection.
        conn.pragma_update(None, "foreign_keys", "ON")?;

        // Briefing 044d: one-time migration from the legacy ratchet_states
        // schema (16 generic columns, never written to). Detect by probing
        // for `root_key` which only existed in the old shape. Drop the
        // table; the SCHEMA execute_batch below will re-create it with the
        // new (contact_id, state_blob, format_version, updated_at) shape.
        // Safe because the legacy schema had zero INSERT statements in the
        // codebase - no production data is lost.
        let has_legacy_ratchet_schema: bool = conn
            .query_row(
                "SELECT 1 FROM pragma_table_info('ratchet_states') WHERE name = 'root_key'",
                [],
                |_| Ok(()),
            )
            .is_ok();
        if has_legacy_ratchet_schema {
            conn.execute_batch("DROP TABLE ratchet_states;")?;
            tracing::info!(
                "044d migration: dropped legacy ratchet_states schema (zero INSERTs ever, no data loss)"
            );
        }

        conn.execute_batch(SCHEMA)?;
        // CREATE TABLE IF NOT EXISTS does NOT add columns to a pre-existing
        // table; add them via ALTER TABLE, ignoring "duplicate column" errors
        // on databases that already have the columns. Keeps old installs
        // usable across schema additions.
        for alter in [
            "ALTER TABLE contacts ADD COLUMN rcv_id              BLOB",
            "ALTER TABLE contacts ADD COLUMN snd_id              BLOB",
            "ALTER TABLE contacts ADD COLUMN rcv_auth_private    BLOB",
            "ALTER TABLE contacts ADD COLUMN rcv_dh_private      BLOB",
            "ALTER TABLE contacts ADD COLUMN rcv_dh_public       BLOB",
            "ALTER TABLE contacts ADD COLUMN snd_auth_private    BLOB",
            "ALTER TABLE contacts ADD COLUMN e2e_key1_private    BLOB",
            "ALTER TABLE contacts ADD COLUMN e2e_key1_public     BLOB",
            "ALTER TABLE contacts ADD COLUMN e2e_key2_private    BLOB",
            "ALTER TABLE contacts ADD COLUMN e2e_key2_public     BLOB",
            "ALTER TABLE contacts ADD COLUMN peer_snd_id         BLOB",
            "ALTER TABLE contacts ADD COLUMN peer_dh_public      BLOB",
            "ALTER TABLE contacts ADD COLUMN msg_id_send         INTEGER DEFAULT 0",
            "ALTER TABLE contacts ADD COLUMN prev_msg_hash       BLOB",
            "ALTER TABLE contacts ADD COLUMN sender_auth_key_private BLOB",
            "ALTER TABLE contacts ADD COLUMN sender_auth_key_public  BLOB",
            "ALTER TABLE contacts ADD COLUMN peer_e2e_pub            BLOB",
            // Briefing 041b-crypto-fix CF3: our own X25519 L2 ephemeral
            // private key generated during Stage 16's AgentConfirmation
            // reply. Peer stores the matching public half via its first-
            // message Layer 2 PubHeader decode; chat sends must reuse
            // this keypair (PubHeader Nothing) so peer's DH derives the
            // same secret. Persisted once per contact, never rotated.
            "ALTER TABLE contacts ADD COLUMN own_l2_ephemeral_private BLOB",
            // Briefing 041b-hello: idempotency flag for the post-handshake
            // outbound HELLO. GoChat sends HELLO exactly once in response
            // to peer's HELLO (`connection.ts:776-785`); we mirror that
            // and guard against double-send across contact-loop restarts
            // or duplicate peer HELLOs.
            "ALTER TABLE contacts ADD COLUMN hello_sent INTEGER NOT NULL DEFAULT 0",
            // Briefing 044c: the X25519 private whose public half was
            // registered with the SMP server as this queue's rcvAuthKey
            // during the NEW command. Used by the SmpConnection layer
            // and re-bound to fresh sessions in reconnect_with_backoff.
            // The column is separate from `rcv_auth_private` (now also
            // live, see 044e) because the two keys serve different
            // protocol layers: queue_auth_private is the X25519 key the
            // server uses to verify session-binding on reconnect;
            // rcv_auth_private is the Ed25519 SigningKey the agent layer
            // uses to sign every SUB/ACK/KEY recipient command. Public
            // half is derived on load via X25519 basepoint-mult, not
            // stored - the Private is the single source of truth.
            "ALTER TABLE contacts ADD COLUMN queue_auth_private BLOB",
            // Briefing 045 W1: fields needed by ListSimplexContacts to
            // produce a rich ContactSummary. Defaults keep pre-045 rows
            // valid: last_message_at stays NULL until a message is
            // observed; unread_count is 0 until the first unacked
            // inbound message. Both columns are updated by future
            // message-pipeline code (receive path on inbound, UI action
            // on mark-as-read).
            "ALTER TABLE contacts ADD COLUMN last_message_at INTEGER",
            "ALTER TABLE contacts ADD COLUMN unread_count INTEGER NOT NULL DEFAULT 0",
            // Briefing 045 W1 corollary: peer profile full_name so the
            // ContactSummary can surface it without another round-trip.
            // Stored during AgentConfirmation processing on Stage 4a
            // (see agent_confirmation.rs), column was previously
            // missing from the contacts schema despite display_name
            // and bio existing on the peer profile payload.
            "ALTER TABLE contacts ADD COLUMN full_name TEXT",
            "ALTER TABLE contacts ADD COLUMN bio TEXT",
        ] {
            // Ignore "duplicate column name" errors on already-migrated DBs.
            let _ = conn.execute(alter, []);
        }

        // Briefing 044g.1a: versioned schema migrations. Pre-migration
        // safety (sanity checks + automatic backup) runs first so a
        // partially-broken DB cannot silently destroy itself; then the
        // migration runner applies whatever is pending. Post-migration
        // FK integrity check catches inconsistencies that the migration
        // SQL might have introduced. Any failure aborts startup with a
        // clear error message.
        Self::pre_migration_safety(&conn, &db_path)?;
        let applied = crate::migrations::apply_pending(&mut conn)
            .context("schema migrations")?;
        if applied > 0 {
            tracing::info!(applied, "schema migrations completed");
            // PRAGMA foreign_key_check returns one row per FK violation.
            // A non-empty result means the migration left dangling
            // references; abort startup so we do not corrupt user data.
            let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
            let violations: Vec<String> = stmt
                .query_map([], |row| {
                    Ok(format!(
                        "table={} rowid={} parent={} fkid={}",
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            if !violations.is_empty() {
                anyhow::bail!(
                    "post-migration FK violations detected: {:?}",
                    violations
                );
            }
        }

        tracing::info!("SimpleX store opened at {:?}", db_path);
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Briefing 044g.1a: pre-migration sanity checks plus automatic backup.
    ///
    /// Three pre-flight checks before the connections-table migration runs:
    /// 1. Skip entirely on a fresh DB (no contacts table → nothing to migrate).
    /// 2. Skip on an already-migrated DB (connections table exists).
    /// 3. Verify no contacts have NULL identity columns; verify no orphan
    ///    ratchet_states rows. Either condition would break the new FKs.
    ///
    /// Then take a timestamped backup at
    /// `simplex.db.pre_044g1a_backup_<unix_ts>` so the user can recover if
    /// the migration corrupts data despite the in-transaction safeguards.
    fn pre_migration_safety(conn: &Connection, db_path: &Path) -> Result<()> {
        let contacts_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='contacts'",
                [],
                |row| row.get::<_, i64>(0).map(|n| n > 0),
            )
            .unwrap_or(false);
        if !contacts_exists {
            tracing::info!("fresh DB, skipping pre-migration safety checks");
            return Ok(());
        }

        let connections_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='connections'",
                [],
                |row| row.get::<_, i64>(0).map(|n| n > 0),
            )
            .unwrap_or(false);
        if connections_exists {
            tracing::debug!("post-migration DB, skipping pre-migration safety checks");
            return Ok(());
        }

        // Pre-044g.1a DB with existing data. Run sanity + backup.
        let contact_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM contacts", [], |row| row.get(0))?;
        tracing::info!(
            contact_count,
            "pre-migration sanity check: contacts in DB"
        );

        // Sanity check 1: corrupted identity columns. The new connections
        // schema requires NOT NULL on server_host and fingerprint; any row
        // with NULL there would fail the migration's INSERT. Surface
        // clearly so the user can either wipe the DB or fix it manually.
        let bad_identity: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT id FROM contacts \
                 WHERE server_host IS NULL OR fingerprint IS NULL",
            )?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<_, _>>()?
        };
        if !bad_identity.is_empty() {
            anyhow::bail!(
                "pre-migration check failed: contacts with NULL server_host or \
                 fingerprint: {:?}. Manual recovery required (wipe contacts or \
                 fix DB) before app can start.",
                bad_identity
            );
        }

        // Sanity check 2: orphan ratchet_states rows. The new FK on
        // ratchet_states references connections.connection_id; an orphan
        // ratchet row without a matching contact (and thus no derived
        // connection) would be silently dropped by the migration's JOIN.
        // Better to surface and abort.
        let orphan_ratchets: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT contact_id FROM ratchet_states \
                 WHERE contact_id NOT IN (SELECT id FROM contacts)",
            )?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<_, _>>()?
        };
        if !orphan_ratchets.is_empty() {
            anyhow::bail!(
                "pre-migration check failed: orphaned ratchet_states for missing \
                 contacts: {:?}. Manual recovery required.",
                orphan_ratchets
            );
        }

        // Backup before migration. Filename embeds the unix timestamp so
        // multiple backup attempts (e.g. a crashed migration followed by
        // a retry) do not overwrite each other.
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let backup_path = db_path.with_file_name(format!(
            "{}.pre_044g1a_backup_{}",
            db_path.file_name().unwrap_or_default().to_string_lossy(),
            ts
        ));
        std::fs::copy(db_path, &backup_path)
            .with_context(|| format!("create pre-migration backup at {:?}", backup_path))?;
        tracing::info!(
            backup_path = %backup_path.display(),
            "created pre-migration backup"
        );

        Ok(())
    }

    /// Save the full user profile to the singleton `user_profile` row.
    ///
    /// On conflict (row exists) updates all mutable columns but leaves
    /// `created_at` unchanged; `updated_at` is bumped to the current time.
    pub fn save_user_profile(&self, profile: &UserProfile) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        conn.execute(
            "INSERT INTO user_profile (id, display_name, full_name, bio, preferences_json, created_at, updated_at)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                 display_name = excluded.display_name,
                 full_name = excluded.full_name,
                 bio = excluded.bio,
                 preferences_json = excluded.preferences_json,
                 updated_at = excluded.updated_at",
            rusqlite::params![
                profile.display_name,
                profile.full_name,
                profile.bio,
                profile.preferences_json,
                now,
                now,
            ],
        )?;
        Ok(())
    }

    /// Load the singleton user profile, or `None` if not yet configured.
    pub fn get_user_profile(&self) -> Result<Option<UserProfile>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT display_name, full_name, bio, preferences_json, created_at, updated_at
             FROM user_profile WHERE id = 1",
        )?;
        let row = stmt
            .query_row([], |row| {
                Ok(UserProfile {
                    display_name: row.get(0)?,
                    full_name: row.get(1)?,
                    bio: row.get(2)?,
                    preferences_json: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })
            .ok();
        Ok(row)
    }

    /// Save or update the user profile display name.
    pub fn save_profile(&self, display_name: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO profile (key, value) VALUES ('display_name', ?1)",
            [display_name],
        )?;
        Ok(())
    }

    /// Get the user profile display name, if set.
    pub fn get_profile_name(&self) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM profile WHERE key = 'display_name'")?;
        let name = stmt.query_row([], |row| row.get::<_, String>(0)).ok();
        Ok(name)
    }

    /// Briefing 044g.1a: resolve a `contact_id` to its currently-active
    /// `connection_id`. Used by Tier-2 wrappers to dispatch contact-keyed
    /// API calls to the underlying connection-keyed primitives.
    ///
    /// Status preference order: Active > Secured > Confirmed > New, with
    /// `Disabled` excluded entirely; ties broken by created_at DESC. For
    /// 1:1 legacy data (one connection per contact) this returns the only
    /// row. For future multi-connection scenarios (post-044g.2) it picks
    /// the most-recently-active queue.
    ///
    /// Returns `Ok(None)` when the contact has no connections at all
    /// (post-wipe state) - caller decides whether that is fatal or
    /// graceful (loaders fall through to None, savers bail).
    pub(crate) fn resolve_active_connection_id(
        &self,
        contact_id: &str,
    ) -> Result<Option<i64>> {
        let conn = self.conn.lock().unwrap();
        Self::resolve_active_connection_id_inner(&conn, contact_id)
    }

    /// Inner form taking a `&Connection`, usable from contexts that
    /// already hold the lock (avoids re-locking the Mutex).
    fn resolve_active_connection_id_inner(
        conn: &Connection,
        contact_id: &str,
    ) -> Result<Option<i64>> {
        let result = conn.query_row(
            "SELECT connection_id FROM connections \
             WHERE contact_id = ?1 \
               AND conn_status != 'Disabled' \
             ORDER BY \
               CASE conn_status \
                 WHEN 'Active'    THEN 0 \
                 WHEN 'Secured'   THEN 1 \
                 WHEN 'Confirmed' THEN 2 \
                 WHEN 'New'       THEN 3 \
                 ELSE 9 \
               END, \
               created_at DESC \
             LIMIT 1",
            rusqlite::params![contact_id],
            |row| row.get::<_, i64>(0),
        );
        match result {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Save a new contact from a parsed invitation link.
    ///
    /// Briefing 044g.1a: now writes TWO rows in one transaction - the
    /// identity row in `contacts` and the queue row in `connections`.
    /// Returns the new `connection_id` so callers (044g.2 boot-spawn,
    /// 044g.1b additional saves) can address the freshly-inserted
    /// connection without round-tripping through `resolve_active_*`.
    /// Today's two call sites in service.rs discard the return value
    /// via `?`; no caller-visible churn.
    pub fn save_contact(
        &self,
        id: &str,
        display_name: Option<&str>,
        server_host: &str,
        server_port: u16,
        fingerprint: &str,
        queue_id: &str,
        sender_key: &str,
    ) -> Result<i64> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        // Identity row: only the user-facing fields stay on contacts
        // post-044g.1a. The pre-migration `status` column moved to
        // connections.conn_status; we no longer write it here.
        // Preserve created_at on conflict so re-running save_contact
        // (which today is INSERT OR REPLACE for idempotency) does not
        // back-date the contact's establishment timestamp.
        tx.execute(
            "INSERT INTO contacts (id, display_name, created_at) \
             VALUES (?1, ?2, unixepoch()) \
             ON CONFLICT(id) DO UPDATE SET \
                 display_name = excluded.display_name",
            rusqlite::params![id, display_name],
        )?;

        // Queue row in connections. The UNIQUE(server_host, server_port,
        // queue_id) constraint preserves the per-queue uniqueness today
        // expressed by the (id, queue_id) shape; on conflict we update
        // sender_key / fingerprint in place. Legacy 1:1 backfill puts
        // the contact_id bytes into agent_conn_id; new connections from
        // 044g.2+ can override this with a fresh UUID.
        let mut stmt = tx.prepare(
            "INSERT INTO connections ( \
                contact_id, agent_conn_id, conn_status, conn_type, \
                server_host, server_port, fingerprint, queue_id, sender_key, \
                created_at, updated_at \
             ) VALUES (?1, ?2, 'New', 'contact', ?3, ?4, ?5, ?6, ?7, unixepoch(), unixepoch()) \
             ON CONFLICT(server_host, server_port, queue_id) DO UPDATE SET \
                 sender_key = excluded.sender_key, \
                 fingerprint = excluded.fingerprint, \
                 updated_at = unixepoch() \
             RETURNING connection_id",
        )?;
        let connection_id: i64 = stmt.query_row(
            rusqlite::params![
                id,
                id.as_bytes(),
                server_host,
                server_port as i64,
                fingerprint,
                queue_id,
                sender_key,
            ],
            |row| row.get(0),
        )?;
        drop(stmt);

        tx.commit()?;
        Ok(connection_id)
    }

    /// List all contacts.
    pub fn list_contacts(&self) -> Result<Vec<ContactRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, display_name, server_host, status, created_at FROM contacts ORDER BY created_at DESC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ContactRow {
                    id: row.get(0)?,
                    display_name: row.get(1)?,
                    server_host: row.get(2)?,
                    status: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// Clear all data (for logout).
    pub fn clear_all(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch("DELETE FROM messages; DELETE FROM contacts; DELETE FROM profile;")?;
        Ok(())
    }

    /// List all contacts with the rich ContactSummary payload needed by
    /// Briefing 045 `ListSimplexContacts` RPC. Sort order puts the
    /// most-recently-active contact first (for natural sidebar display):
    /// last_message_at first (NULL ranked last), then established_at
    /// descending as a deterministic tie-breaker for newly created
    /// contacts that have not yet exchanged a message.
    pub fn list_all_contacts(&self) -> Result<Vec<ContactSummaryRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, display_name, full_name, created_at, last_message_at, unread_count
             FROM contacts
             ORDER BY last_message_at DESC NULLS LAST, created_at DESC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ContactSummaryRow {
                    contact_id: row.get(0)?,
                    display_name: row.get(1)?,
                    full_name: row.get(2)?,
                    // created_at is the original contact-creation unix time
                    // and is a stable proxy for "established_at" since the
                    // row is only inserted after a successful handshake.
                    established_at_unix: row.get(3)?,
                    last_message_at_unix: row.get(4)?,
                    unread_count: row.get(5).unwrap_or(0),
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// Destructive wipe of all contacts and their directly-linked crypto
    /// state. Leaves the singleton `user_profile` row intact so the
    /// Wizard's subsequent `SetProfile` can reuse it.
    ///
    /// Tables cleared:
    /// - `contacts` (including the queue_auth_private, peer_e2e_pub,
    ///   sender_auth_key_*, own_l2_ephemeral_private columns)
    /// - `messages` (FK on contact_id)
    /// - `ratchet_states` (contact_id PK)
    /// - `sender_auth` (contact_id PK)
    ///
    /// Ordering: child rows first, parent last, to respect the REFERENCES
    /// constraint on messages.contact_id. A single transaction keeps the
    /// operation atomic from the SQL layer's point of view.
    ///
    /// Returns the number of contact rows that were removed.
    ///
    /// Briefing 045 W5: called by `WipeAllSimplexContacts` RPC which is
    /// gated behind the Wizard's explicit orphan-cleanup confirmation and
    /// a `wizard_intent=true` flag on the Tauri wrapper. No other caller
    /// should invoke this method.
    pub fn wipe_all_contacts(&self) -> Result<u32> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let count: i64 =
            tx.query_row("SELECT COUNT(*) FROM contacts", [], |row| row.get(0))?;
        // Briefing 044g.1a: explicit DELETE FROM connections before contacts.
        // FK CASCADE on connections.contact_id and ratchet_states.connection_id
        // would cover this implicitly; the explicit DELETEs preserve the
        // belt-and-braces style of the original wipe and clarify intent.
        tx.execute_batch(
            "DELETE FROM messages;\
             DELETE FROM ratchet_states;\
             DELETE FROM sender_auth;\
             DELETE FROM connections;\
             DELETE FROM contacts;",
        )?;
        tx.commit()?;
        Ok(count as u32)
    }

    /// Save proxy configuration.
    pub fn save_proxy(&self, host: &str, port: i32) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO profile (key, value) VALUES ('proxy', ?1)",
            [format!("{host}:{port}")],
        )?;
        Ok(())
    }

    /// Clear proxy configuration.
    pub fn clear_proxy(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM profile WHERE key = 'proxy'", [])?;
        Ok(())
    }

    // ---- Handshake state persistence ----

    // Briefing 044e: the previous `save_handshake_keys` helper that wrote
    // a six-column UPDATE on `contacts` (rcv_id, snd_id, rcv_auth_private,
    // rcv_dh_private, rcv_dh_public, snd_auth_private) lived here as
    // dead-code; no production caller ever invoked it. Replaced by the
    // single-purpose `save_rcv_auth_private` further down, which mirrors
    // 044c's `save_queue_auth_private` shape and is wired into both
    // handshake paths in service.rs. The other five columns of the old
    // helper either still need their own per-field savers (followup
    // briefings) or are not actually persisted today.

    /// Briefing 044g.1a Tier-1: save E2E keypairs by connection_id.
    #[allow(dead_code)]
    pub(crate) fn save_e2e_keypairs_for_connection(
        &self,
        connection_id: i64,
        key1_private: &[u8],
        key1_public: &[u8],
        key2_private: &[u8],
        key2_public: &[u8],
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE connections SET e2e_key1_private=?2, e2e_key1_public=?3, \
             e2e_key2_private=?4, e2e_key2_public=?5, updated_at=unixepoch() \
             WHERE connection_id=?1",
            rusqlite::params![
                connection_id,
                key1_private,
                key1_public,
                key2_private,
                key2_public
            ],
        )?;
        Ok(())
    }

    /// Save X448 E2E keypairs generated for AgentInvitation.
    /// Briefing 044g.1a Tier-2: contact-keyed wrapper around the
    /// connection-keyed primitive.
    #[allow(dead_code)]
    pub fn save_e2e_keypairs(
        &self,
        contact_id: &str,
        key1_private: &[u8],
        key1_public: &[u8],
        key2_private: &[u8],
        key2_public: &[u8],
    ) -> Result<()> {
        let connection_id = self
            .resolve_active_connection_id(contact_id)?
            .ok_or_else(|| {
                anyhow::anyhow!("no active connection for contact {}", contact_id)
            })?;
        self.save_e2e_keypairs_for_connection(
            connection_id,
            key1_private,
            key1_public,
            key2_private,
            key2_public,
        )
    }

    /// Wipe all SimpleX-local state: profile, user_profile, contacts,
    /// messages, ratchet_states, sender_auth. Leaves the SQLite schema
    /// intact so the sidecar keeps working without restart; the user can
    /// set up a fresh profile and new contacts afterwards.
    ///
    /// Used by the ResetSimplex gRPC endpoint (Settings > Disconnect).
    pub fn reset_all(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        // Briefing 044g.1a: include DELETE FROM connections in the wipe
        // sequence. Order matches FK dependencies (children before parents).
        conn.execute_batch(
            "DELETE FROM user_profile;\n\
             DELETE FROM profile;\n\
             DELETE FROM sender_auth;\n\
             DELETE FROM ratchet_states;\n\
             DELETE FROM messages;\n\
             DELETE FROM connections;\n\
             DELETE FROM contacts;",
        )?;
        Ok(())
    }

    /// Briefing 044g.1a Tier-1: save peer e2e pub by connection_id.
    #[allow(dead_code)]
    pub(crate) fn save_peer_e2e_pub_for_connection(
        &self,
        connection_id: i64,
        pub_key: &[u8; 32],
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE connections SET peer_e2e_pub=?2, updated_at=unixepoch() \
             WHERE connection_id=?1",
            rusqlite::params![connection_id, &pub_key[..]],
        )?;
        Ok(())
    }

    /// Save the peer's X25519 ephemeral DH public key observed in the
    /// PubHeader of the first incoming message. Subsequent messages from
    /// the same peer use the `Maybe Nothing` PubHeader variant and rely on
    /// this stored key to recompute the per-queue DH secret.
    /// Briefing 044g.1a Tier-2 wrapper.
    #[allow(dead_code)]
    pub fn save_peer_e2e_pub(&self, contact_id: &str, pub_key: &[u8; 32]) -> Result<()> {
        let connection_id = self
            .resolve_active_connection_id(contact_id)?
            .ok_or_else(|| {
                anyhow::anyhow!("no active connection for contact {}", contact_id)
            })?;
        self.save_peer_e2e_pub_for_connection(connection_id, pub_key)
    }

    /// Briefing 044g.1a Tier-1: load peer e2e pub by connection_id.
    #[allow(dead_code)]
    pub(crate) fn load_peer_e2e_pub_for_connection(
        &self,
        connection_id: i64,
    ) -> Result<Option<[u8; 32]>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT peer_e2e_pub FROM connections WHERE connection_id=?1")?;
        let result: Option<Vec<u8>> = stmt
            .query_row([connection_id], |row| row.get::<_, Option<Vec<u8>>>(0))
            .ok()
            .flatten();
        match result {
            Some(bytes) if bytes.len() == 32 => {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&bytes);
                Ok(Some(arr))
            }
            _ => Ok(None),
        }
    }

    /// Load the stored peer X25519 ephemeral DH public key for Layer 2
    /// decryption of subsequent messages with `Maybe Nothing` PubHeader.
    /// Briefing 044g.1a Tier-2 wrapper.
    #[allow(dead_code)]
    pub fn load_peer_e2e_pub(&self, contact_id: &str) -> Result<Option<[u8; 32]>> {
        let Some(connection_id) = self.resolve_active_connection_id(contact_id)? else {
            return Ok(None);
        };
        self.load_peer_e2e_pub_for_connection(connection_id)
    }

    /// Briefing 044g.1a Tier-1: save sender auth keypair by connection_id.
    #[allow(dead_code)]
    pub(crate) fn save_sender_auth_keypair_for_connection(
        &self,
        connection_id: i64,
        private: &[u8],
        public_spki: &[u8],
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE connections SET sender_auth_key_private=?2, \
             sender_auth_key_public=?3, updated_at=unixepoch() \
             WHERE connection_id=?1",
            rusqlite::params![connection_id, private, public_spki],
        )?;
        Ok(())
    }

    /// Save the X25519 sender auth keypair generated during the invitation
    /// handshake response. The public SPKI is embedded in the PHConfirmation
    /// header sent to the peer's reply queue; the private key is kept for
    /// future signed SEND commands once the peer secures the queue via KEY.
    /// Briefing 044g.1a Tier-2 wrapper.
    #[allow(dead_code)]
    pub fn save_sender_auth_keypair(
        &self,
        contact_id: &str,
        private: &[u8],
        public_spki: &[u8],
    ) -> Result<()> {
        let connection_id = self
            .resolve_active_connection_id(contact_id)?
            .ok_or_else(|| {
                anyhow::anyhow!("no active connection for contact {}", contact_id)
            })?;
        self.save_sender_auth_keypair_for_connection(connection_id, private, public_spki)
    }

    /// Briefing 044g.1a Tier-1: load sender_auth_private by connection_id.
    #[allow(dead_code)]
    pub(crate) fn load_sender_auth_private_for_connection(
        &self,
        connection_id: i64,
    ) -> Result<Option<[u8; 32]>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT sender_auth_key_private FROM connections WHERE connection_id=?1",
        )?;
        let bytes: Option<Vec<u8>> = stmt
            .query_row([connection_id], |row| row.get::<_, Option<Vec<u8>>>(0))
            .ok()
            .flatten();
        match bytes {
            Some(b) if b.len() == 32 => {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&b);
                Ok(Some(arr))
            }
            _ => Ok(None),
        }
    }

    /// Load the X25519 sender auth private key for signing SEND commands on
    /// the peer's reply queue. The queue is secured by the peer via SKEY
    /// using our public half (registered during handshake), so each
    /// subsequent SEND must carry a crypto_box MAC computed with this key
    /// against the server's session DH pub key.
    ///
    /// Returns `Ok(None)` when no keypair has been persisted for this
    /// contact yet (e.g. handshake incomplete or the contact was seeded
    /// pre-Briefing 041b-fix).
    /// Briefing 044g.1a Tier-2 wrapper.
    pub fn load_sender_auth_private(&self, contact_id: &str) -> Result<Option<[u8; 32]>> {
        let Some(connection_id) = self.resolve_active_connection_id(contact_id)? else {
            return Ok(None);
        };
        self.load_sender_auth_private_for_connection(connection_id)
    }

    /// Briefing 044g.1a Tier-1: save own L2 ephemeral by connection_id.
    pub(crate) fn save_own_l2_ephemeral_private_for_connection(
        &self,
        connection_id: i64,
        private: &[u8; 32],
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE connections SET own_l2_ephemeral_private=?2, updated_at=unixepoch() \
             WHERE connection_id=?1",
            rusqlite::params![connection_id, &private[..]],
        )?;
        Ok(())
    }

    /// Save the X25519 L2 ephemeral private key we generated for the
    /// Layer 2 NaCl envelope on Stage 16 (AgentConfirmation reply to
    /// peer's reply queue). The peer stored the matching public half
    /// when it decoded our first-message PubHeader (`Just` variant with
    /// inline DH pub). Post-handshake chat sends send PubHeader
    /// `Nothing` and rely on the peer's stored key, so we must reuse
    /// THIS specific private half for the DH to land on the same shared
    /// secret. Persisted once per contact, never rotated.
    /// Briefing 044g.1a Tier-2 wrapper.
    pub fn save_own_l2_ephemeral_private(
        &self,
        contact_id: &str,
        private: &[u8; 32],
    ) -> Result<()> {
        let connection_id = self
            .resolve_active_connection_id(contact_id)?
            .ok_or_else(|| {
                anyhow::anyhow!("no active connection for contact {}", contact_id)
            })?;
        self.save_own_l2_ephemeral_private_for_connection(connection_id, private)
    }

    /// Briefing 044g.1a Tier-1: load own L2 ephemeral by connection_id.
    pub(crate) fn load_own_l2_ephemeral_private_for_connection(
        &self,
        connection_id: i64,
    ) -> Result<Option<[u8; 32]>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT own_l2_ephemeral_private FROM connections WHERE connection_id=?1",
        )?;
        let bytes: Option<Vec<u8>> = stmt
            .query_row([connection_id], |row| row.get::<_, Option<Vec<u8>>>(0))
            .ok()
            .flatten();
        match bytes {
            Some(b) if b.len() == 32 => {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&b);
                Ok(Some(arr))
            }
            _ => Ok(None),
        }
    }

    /// Load the X25519 L2 ephemeral private key from Stage 16. Returns
    /// `Ok(None)` when the column is empty (handshake not completed, or
    /// contact pre-dates Briefing 041b-crypto-fix).
    /// Briefing 044g.1a Tier-2 wrapper.
    pub fn load_own_l2_ephemeral_private(
        &self,
        contact_id: &str,
    ) -> Result<Option<[u8; 32]>> {
        let Some(connection_id) = self.resolve_active_connection_id(contact_id)? else {
            return Ok(None);
        };
        self.load_own_l2_ephemeral_private_for_connection(connection_id)
    }

    /// Mark the contact as having had its outbound HELLO delivered to
    /// the peer's reply queue. Called once after a successful HELLO
    /// SEND on the contact-session background loop. Idempotent: calling
    /// twice is harmless but a no-op on the second call.
    /// Briefing 044g.1a Tier-2 wrapper.
    pub fn set_hello_sent(&self, contact_id: &str) -> Result<()> {
        let connection_id = self
            .resolve_active_connection_id(contact_id)?
            .ok_or_else(|| {
                anyhow::anyhow!("no active connection for contact {}", contact_id)
            })?;
        self.set_hello_sent_for_connection(connection_id)
    }

    /// Briefing 044g.1a Tier-1: set hello_sent by connection_id.
    pub(crate) fn set_hello_sent_for_connection(&self, connection_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE connections SET hello_sent=1, updated_at=unixepoch() \
             WHERE connection_id=?1",
            rusqlite::params![connection_id],
        )?;
        Ok(())
    }

    /// Read the `hello_sent` flag. Returns `false` for contacts that
    /// pre-date Briefing 041b-hello (the column defaults to 0) or whose
    /// HELLO has not yet been dispatched successfully.
    /// Briefing 044g.1a Tier-2 wrapper.
    pub fn get_hello_sent(&self, contact_id: &str) -> Result<bool> {
        let Some(connection_id) = self.resolve_active_connection_id(contact_id)? else {
            return Ok(false);
        };
        self.get_hello_sent_for_connection(connection_id)
    }

    /// Briefing 044g.1a Tier-1: read hello_sent by connection_id.
    pub(crate) fn get_hello_sent_for_connection(&self, connection_id: i64) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT hello_sent FROM connections WHERE connection_id=?1")?;
        let flag: Option<i64> = stmt
            .query_row([connection_id], |row| row.get::<_, Option<i64>>(0))
            .ok()
            .flatten();
        Ok(matches!(flag, Some(v) if v != 0))
    }

    /// Persist the queue_auth X25519 private key whose public half was
    /// registered with the SMP server as `rcvAuthKey` during the NEW
    /// command (Briefing 044c).
    ///
    /// Called once per contact, immediately after the IDS response from
    /// NEW confirms the queue is alive on the server. The stored value is
    /// the single source of truth for recipient-side command auth across
    /// TCP/TLS reconnects: on each reconnect, `reconnect_with_backoff`
    /// loads this key and patches it into the fresh `SmpConnection`
    /// before issuing the re-SUB, so the server's registered public key
    /// still validates the crypto_box MAC.
    ///
    /// The public half is NOT stored alongside - it is derived on load
    /// via X25519 basepoint mult, guaranteeing consistency by
    /// construction.
    /// Briefing 044g.1a Tier-2 wrapper.
    pub fn save_queue_auth_private(
        &self,
        contact_id: &str,
        private: &[u8; 32],
    ) -> Result<()> {
        let connection_id = self
            .resolve_active_connection_id(contact_id)?
            .ok_or_else(|| {
                anyhow::anyhow!("no active connection for contact {}", contact_id)
            })?;
        self.save_queue_auth_private_for_connection(connection_id, private)
    }

    /// Briefing 044g.1a Tier-1: save queue_auth_private by connection_id.
    pub(crate) fn save_queue_auth_private_for_connection(
        &self,
        connection_id: i64,
        private: &[u8; 32],
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE connections SET queue_auth_private=?2, updated_at=unixepoch() \
             WHERE connection_id=?1",
            rusqlite::params![connection_id, &private[..]],
        )?;
        Ok(())
    }

    /// Briefing 044e: persist the Ed25519 SigningKey seed used to sign
    /// SMP recipient commands (SUB / ACK / KEY) for this contact. The
    /// key is generated once per contact during the handshake and never
    /// rotates; saved at end-of-handshake and consumed by 044g's
    /// boot-time spawn loop to reconstruct a `SigningKey` via
    /// `ed25519_dalek::SigningKey::from_bytes(&seed)`.
    ///
    /// Companion to `save_queue_auth_private` from 044c. Both keys are
    /// recipient-side credentials but bind to different protocol layers:
    /// queue_auth_private (X25519) for the SmpConnection session layer,
    /// rcv_auth_private (Ed25519 seed) for the agent-layer command auth.
    /// Briefing 044g.1a Tier-2 wrapper.
    pub fn save_rcv_auth_private(
        &self,
        contact_id: &str,
        private: &[u8; 32],
    ) -> Result<()> {
        let connection_id = self
            .resolve_active_connection_id(contact_id)?
            .ok_or_else(|| {
                anyhow::anyhow!("no active connection for contact {}", contact_id)
            })?;
        self.save_rcv_auth_private_for_connection(connection_id, private)
    }

    /// Briefing 044g.1a Tier-1: save rcv_auth_private by connection_id.
    pub(crate) fn save_rcv_auth_private_for_connection(
        &self,
        connection_id: i64,
        private: &[u8; 32],
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE connections SET rcv_auth_private=?2, updated_at=unixepoch() \
             WHERE connection_id=?1",
            rusqlite::params![connection_id, &private[..]],
        )?;
        Ok(())
    }

    /// Briefing 044e: load the persisted Ed25519 SigningKey seed.
    /// Returns `Ok(None)` when the column is NULL: either the contact
    /// pre-dates 044e (handshake ran before the save was wired) or the
    /// handshake itself never reached the save checkpoint. Callers in
    /// 044g treat `None` as "respawn impossible without manual
    /// re-establish" - the matching public half is registered on the
    /// SMP server and cannot be regenerated client-side.
    ///
    /// Only consumed by the 044g boot-time respawn path. Tests in this
    /// module exercise the function so the dead-code lint stays quiet.
    /// Briefing 044g.1a Tier-2 wrapper.
    #[allow(dead_code)]
    pub fn load_rcv_auth_private(
        &self,
        contact_id: &str,
    ) -> Result<Option<[u8; 32]>> {
        let Some(connection_id) = self.resolve_active_connection_id(contact_id)? else {
            return Ok(None);
        };
        self.load_rcv_auth_private_for_connection(connection_id)
    }

    /// Briefing 044g.1a Tier-1: load rcv_auth_private by connection_id.
    #[allow(dead_code)]
    pub(crate) fn load_rcv_auth_private_for_connection(
        &self,
        connection_id: i64,
    ) -> Result<Option<[u8; 32]>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT rcv_auth_private FROM connections WHERE connection_id=?1")?;
        let bytes: Option<Vec<u8>> = stmt
            .query_row([connection_id], |row| row.get::<_, Option<Vec<u8>>>(0))
            .ok()
            .flatten();
        match bytes {
            Some(b) if b.len() == 32 => {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&b);
                Ok(Some(arr))
            }
            _ => Ok(None),
        }
    }

    /// Briefing 044d: persist the postcard-encoded ratchet state for a
    /// contact. Called from the BG-loop's centralised dirty-flag check at
    /// the end of each select-arm iteration. INSERT-OR-UPDATE semantics
    /// mirror `save_queue_auth_private` from 044c: the first save creates
    /// the row, every subsequent call overwrites the blob in place.
    ///
    /// `state_blob` is the output of `PersistedRatchetV::encode`.
    /// `format_version` should be `PersistedRatchetV::current_version()`
    /// at the time of the save; the loader uses it to pick the right
    /// decode path.
    /// Briefing 044g.1a Tier-2 wrapper. ratchet_states.connection_id is
    /// now the FK target; resolve from contact_id at the surface.
    pub fn save_ratchet_state(
        &self,
        contact_id: &str,
        state_blob: &[u8],
        format_version: i64,
    ) -> Result<()> {
        let connection_id = self
            .resolve_active_connection_id(contact_id)?
            .ok_or_else(|| {
                anyhow::anyhow!("no active connection for contact {}", contact_id)
            })?;
        self.save_ratchet_state_for_connection(connection_id, state_blob, format_version)
    }

    /// Briefing 044g.1a Tier-1: save ratchet state by connection_id.
    pub(crate) fn save_ratchet_state_for_connection(
        &self,
        connection_id: i64,
        state_blob: &[u8],
        format_version: i64,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO ratchet_states(connection_id, state_blob, format_version) \
             VALUES (?1, ?2, ?3) \
             ON CONFLICT(connection_id) DO UPDATE SET \
                state_blob=excluded.state_blob, \
                format_version=excluded.format_version, \
                updated_at=unixepoch()",
            rusqlite::params![connection_id, state_blob, format_version],
        )?;
        Ok(())
    }

    /// Briefing 044d: load the persisted ratchet state for a contact.
    /// Returns `Ok(None)` for a contact that has no ratchet row yet
    /// (handshake completed but not yet ratchet-mutated, or pre-044d
    /// contact whose state was lost on the first restart). Returns the
    /// raw blob plus version so callers route to the right decode path
    /// in `PersistedRatchetV::decode`.
    ///
    /// Only used on the 044g boot-time respawn path. Tests in this
    /// module exercise the function so it does not rot.
    /// Briefing 044g.1a Tier-2 wrapper.
    #[allow(dead_code)]
    pub fn load_ratchet_state(
        &self,
        contact_id: &str,
    ) -> Result<Option<(Vec<u8>, i64)>> {
        let Some(connection_id) = self.resolve_active_connection_id(contact_id)? else {
            return Ok(None);
        };
        self.load_ratchet_state_for_connection(connection_id)
    }

    /// Briefing 044g.1a Tier-1: load ratchet state by connection_id.
    #[allow(dead_code)]
    pub(crate) fn load_ratchet_state_for_connection(
        &self,
        connection_id: i64,
    ) -> Result<Option<(Vec<u8>, i64)>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT state_blob, format_version FROM ratchet_states \
             WHERE connection_id = ?1",
            rusqlite::params![connection_id],
            |row| {
                let blob: Vec<u8> = row.get(0)?;
                let version: i64 = row.get(1)?;
                Ok((blob, version))
            },
        );
        match result {
            Ok(pair) => Ok(Some(pair)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Load the queue_auth private key for signing recipient-side SMP
    /// commands after reconnect (Briefing 044c).
    ///
    /// Returns `Ok(None)` when the column is NULL - either because the
    /// contact pre-dates Briefing 044c (handshake ran with the old code
    /// path that never persisted this key) or because the handshake
    /// itself did not reach the NEW-success checkpoint. Callers treat
    /// `None` as a hard fail for reconnect: without this key, no valid
    /// SUB can be signed, and the user must re-establish the contact
    /// manually.
    /// Briefing 044g.1a Tier-2 wrapper.
    pub fn load_queue_auth_private(
        &self,
        contact_id: &str,
    ) -> Result<Option<[u8; 32]>> {
        let Some(connection_id) = self.resolve_active_connection_id(contact_id)? else {
            return Ok(None);
        };
        self.load_queue_auth_private_for_connection(connection_id)
    }

    /// Briefing 044g.1a Tier-1: load queue_auth_private by connection_id.
    pub(crate) fn load_queue_auth_private_for_connection(
        &self,
        connection_id: i64,
    ) -> Result<Option<[u8; 32]>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT queue_auth_private FROM connections WHERE connection_id=?1")?;
        let bytes: Option<Vec<u8>> = stmt
            .query_row([connection_id], |row| row.get::<_, Option<Vec<u8>>>(0))
            .ok()
            .flatten();
        match bytes {
            Some(b) if b.len() == 32 => {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&b);
                Ok(Some(arr))
            }
            _ => Ok(None),
        }
    }

    /// Load saved X448 E2E keypairs for X3DH.
    /// Briefing 044g.1a Tier-2 wrapper.
    #[allow(dead_code)]
    pub fn load_e2e_keypairs(
        &self,
        contact_id: &str,
    ) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>)> {
        let connection_id = self
            .resolve_active_connection_id(contact_id)?
            .ok_or_else(|| {
                anyhow::anyhow!("no active connection for contact {}", contact_id)
            })?;
        self.load_e2e_keypairs_for_connection(connection_id)
    }

    /// Briefing 044g.1a Tier-1: load X448 E2E keypairs by connection_id.
    #[allow(dead_code)]
    pub(crate) fn load_e2e_keypairs_for_connection(
        &self,
        connection_id: i64,
    ) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>)> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT e2e_key1_private, e2e_key1_public, e2e_key2_private, e2e_key2_public \
             FROM connections WHERE connection_id=?1",
        )?;
        let row = stmt.query_row([connection_id], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Vec<u8>>(3)?,
            ))
        })?;
        Ok(row)
    }

    /// Update contact's active-connection status.
    /// Briefing 044g.1a Tier-2: contacts.status moved to connections.conn_status;
    /// this wrapper preserves the old surface for unchanged callers.
    /// Briefing 044g.1a-fix1: normalises legacy free-form values
    /// ('pending', 'pending_hello', 'connected') to the canonical
    /// vocabulary before storage so resolve_active_connection_id's
    /// CASE statement classifies them correctly.
    pub fn set_contact_status(&self, contact_id: &str, status: &str) -> Result<()> {
        let normalized = normalize_legacy_status(status);
        let connection_id = self
            .resolve_active_connection_id(contact_id)?
            .ok_or_else(|| {
                anyhow::anyhow!("no active connection for contact {}", contact_id)
            })?;
        self.set_connection_status_for_connection(connection_id, normalized)
    }

    /// Briefing 044g.1a Tier-1: set conn_status by connection_id.
    pub(crate) fn set_connection_status_for_connection(
        &self,
        connection_id: i64,
        status: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE connections SET conn_status=?2, updated_at=unixepoch() \
             WHERE connection_id=?1",
            rusqlite::params![connection_id, status],
        )?;
        Ok(())
    }

    // ==================== Briefing 044g.2: boot-spawn discovery ====================

    /// Returns the connection rows that should be re-subscribed at sidecar
    /// boot. Enriched with server addr / fingerprint to avoid N+1 round-trips
    /// in the per-contact spawn task.
    ///
    /// Filter: `to_subscribe=1`, `conn_status != 'Disabled'`, plus all six
    /// 044g.1b persistence fields IS NOT NULL. The SQL filter is the primary
    /// gate; the per-contact spawn task additionally uses
    /// `load_handshake_persistence_fields` which double-checks via
    /// `Option<...>` for any inconsistency between the WHERE filter and the
    /// row's actual content.
    ///
    /// Ordering: by `connection_id ASC` for deterministic boot order.
    pub fn list_subscribable_connections(&self) -> Result<Vec<SubscribableConnection>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT connection_id, contact_id, server_host, server_port, fingerprint \
             FROM connections \
             WHERE to_subscribe = 1 \
               AND conn_status != 'Disabled' \
               AND rcv_id IS NOT NULL \
               AND rcv_dh_private IS NOT NULL \
               AND srv_dh_public IS NOT NULL \
               AND queue_auth_private IS NOT NULL \
               AND rcv_auth_private IS NOT NULL \
               AND peer_queue_blob IS NOT NULL \
             ORDER BY connection_id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SubscribableConnection {
                connection_id: row.get(0)?,
                contact_id: row.get(1)?,
                server_host: row.get(2)?,
                server_port: row.get::<_, i64>(3)? as u16,
                fingerprint: row.get(4)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    // ==================== Briefing 044g.1b: handshake persistence ====================

    /// Persist the receive-queue identifier (`rcv_id`) returned by the SMP
    /// server in the IDS response. 24 bytes today; column type is BLOB
    /// without a fixed length so future SMP versions could carry larger
    /// IDs without a schema change. Caller is responsible for passing the
    /// canonical 24-byte slice.
    #[allow(dead_code)]
    pub(crate) fn save_rcv_id_for_connection(
        &self,
        connection_id: i64,
        rcv_id: &[u8],
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE connections SET rcv_id = ?2, updated_at = unixepoch() \
             WHERE connection_id = ?1",
            rusqlite::params![connection_id, rcv_id],
        )?;
        Ok(())
    }

    /// Persist the X25519 private key generated locally for queue-level
    /// Diffie-Hellman with the SMP server. Pairs with `srv_dh_public`
    /// (returned by NEW); both halves combine to derive the Layer 3
    /// shared secret used by `decrypt_layer3`.
    #[allow(dead_code)]
    pub(crate) fn save_rcv_dh_private_for_connection(
        &self,
        connection_id: i64,
        rcv_dh_private: &[u8; 32],
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE connections SET rcv_dh_private = ?2, updated_at = unixepoch() \
             WHERE connection_id = ?1",
            rusqlite::params![connection_id, &rcv_dh_private[..]],
        )?;
        Ok(())
    }

    /// Persist the server's X25519 DH public key for this queue. Returned
    /// in the NEW response's IDS frame. Queue-bound (not session-bound):
    /// stays valid across reconnects, so we persist once at handshake
    /// completion and reuse forever.
    #[allow(dead_code)]
    pub(crate) fn save_srv_dh_public_for_connection(
        &self,
        connection_id: i64,
        srv_dh_public: &[u8; 32],
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE connections SET srv_dh_public = ?2, updated_at = unixepoch() \
             WHERE connection_id = ?1",
            rusqlite::params![connection_id, &srv_dh_public[..]],
        )?;
        Ok(())
    }

    /// Persist the peer's reply queue (postcard-encoded SmpQueueInfo).
    /// Mirrors the BobRatchet blob+version pattern from 044d so future
    /// SmpQueueInfo field additions are migration-free in SQL.
    #[allow(dead_code)]
    pub(crate) fn save_peer_queue_for_connection(
        &self,
        connection_id: i64,
        peer_queue: &crate::protocol::smp_queue_info::SmpQueueInfo,
    ) -> Result<()> {
        let blob = postcard::to_allocvec(peer_queue)
            .map_err(|e| anyhow::anyhow!("postcard encode SmpQueueInfo: {e}"))?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE connections \
             SET peer_queue_blob = ?2, peer_queue_format_version = 1, \
                 updated_at = unixepoch() \
             WHERE connection_id = ?1",
            rusqlite::params![connection_id, blob],
        )?;
        Ok(())
    }

    /// Atomic four-field persistence at handshake completion. Wraps all
    /// four UPDATE clauses in a single SQL statement under one
    /// transaction so partial-write inconsistency is impossible. Also
    /// sets `to_subscribe = 1` since a freshly-handshook connection
    /// should be picked up by 044g.2's boot-spawn loop on next start.
    ///
    /// Called once per Contact-Address handshake from Stage 16 in the
    /// BG-loop, immediately after `peer_queue_owned` is parsed from
    /// AgentConnInfoReply. If this fails, the handshake aborts and
    /// returns Err to the user - we refuse to leave a contact in a state
    /// where send works in-RAM but boot-spawn would silently break.
    pub fn save_handshake_persistence_fields(
        &self,
        connection_id: i64,
        rcv_id: &[u8],
        rcv_dh_private: &[u8; 32],
        srv_dh_public: &[u8; 32],
        peer_queue: &crate::protocol::smp_queue_info::SmpQueueInfo,
    ) -> Result<()> {
        let blob = postcard::to_allocvec(peer_queue)
            .map_err(|e| anyhow::anyhow!("postcard encode SmpQueueInfo: {e}"))?;
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE connections \
             SET rcv_id = ?2, \
                 rcv_dh_private = ?3, \
                 srv_dh_public = ?4, \
                 peer_queue_blob = ?5, \
                 peer_queue_format_version = 1, \
                 to_subscribe = 1, \
                 updated_at = unixepoch() \
             WHERE connection_id = ?1",
            rusqlite::params![
                connection_id,
                rcv_id,
                &rcv_dh_private[..],
                &srv_dh_public[..],
                blob,
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Loader for 044g.2's boot-spawn loop. Returns `Ok(None)` if any
    /// of the four required fields is NULL on the row (legacy 044g.1a
    /// backfilled rows have all four NULL). Returns `Ok(Some(_))` only
    /// when all four are present and the postcard decode succeeds.
    /// Returns `Err` only on SQL errors or postcard decode failure.
    #[allow(dead_code)]
    pub fn load_handshake_persistence_fields(
        &self,
        connection_id: i64,
    ) -> Result<Option<HandshakePersistedFields>> {
        let conn = self.conn.lock().unwrap();
        let row: Option<(
            Option<Vec<u8>>,
            Option<Vec<u8>>,
            Option<Vec<u8>>,
            Option<Vec<u8>>,
        )> = conn
            .query_row(
                "SELECT rcv_id, rcv_dh_private, srv_dh_public, peer_queue_blob \
                 FROM connections WHERE connection_id = ?1",
                rusqlite::params![connection_id],
                |r| {
                    Ok((
                        r.get::<_, Option<Vec<u8>>>(0)?,
                        r.get::<_, Option<Vec<u8>>>(1)?,
                        r.get::<_, Option<Vec<u8>>>(2)?,
                        r.get::<_, Option<Vec<u8>>>(3)?,
                    ))
                },
            )
            .ok();
        let Some((Some(rcv_id), Some(dh_priv), Some(srv_dh_pub), Some(blob))) = row else {
            return Ok(None);
        };
        if dh_priv.len() != 32 || srv_dh_pub.len() != 32 {
            anyhow::bail!(
                "load_handshake_persistence_fields: unexpected key length \
                 (rcv_dh_private={}, srv_dh_public={})",
                dh_priv.len(),
                srv_dh_pub.len()
            );
        }
        let mut rcv_dh_private = [0u8; 32];
        rcv_dh_private.copy_from_slice(&dh_priv);
        let mut srv_dh_public_arr = [0u8; 32];
        srv_dh_public_arr.copy_from_slice(&srv_dh_pub);
        let peer_queue: crate::protocol::smp_queue_info::SmpQueueInfo =
            postcard::from_bytes(&blob)
                .map_err(|e| anyhow::anyhow!("postcard decode SmpQueueInfo: {e}"))?;
        Ok(Some(HandshakePersistedFields {
            rcv_id,
            rcv_dh_private,
            srv_dh_public: srv_dh_public_arr,
            peer_queue,
        }))
    }

    /// Briefing 044g.1a-fix1: persist the peer profile fields received in
    /// the post-handshake AgentConnInfo (Stage 15 in the BG-loop). Before
    /// this fix, peer_display_name + peer_full_name were extracted into a
    /// ContactEstablished frontend event but never written to the DB;
    /// after a sidecar restart the sidebar fell back to "SimpleX contact"
    /// because the DB row had display_name=NULL.
    ///
    /// All three fields are identity-side and stay on the `contacts`
    /// table after the 044g.1a connections-refactor. Only updates the
    /// row's display_name / full_name / bio columns; nothing on
    /// connections is touched.
    pub fn update_contact_profile(
        &self,
        contact_id: &str,
        display_name: &str,
        full_name: &str,
        bio: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE contacts SET display_name=?2, full_name=?3, bio=?4 WHERE id=?1",
            rusqlite::params![contact_id, display_name, full_name, bio],
        )?;
        Ok(())
    }

    /// Save an incoming message.
    #[allow(dead_code)]
    pub fn save_incoming_message(&self, contact_id: &str, body: &str) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO messages (contact_id, body, direction) VALUES (?1, ?2, 'in')",
            rusqlite::params![contact_id, body],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Save an outgoing message.
    #[allow(dead_code)]
    pub fn save_outgoing_message(&self, contact_id: &str, body: &str) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO messages (contact_id, body, direction) VALUES (?1, ?2, 'out')",
            rusqlite::params![contact_id, body],
        )?;
        Ok(conn.last_insert_rowid())
    }
}

/// Briefing 044g.1b: bundle of the four handshake-completion fields
/// returned by `load_handshake_persistence_fields`. 044g.2's boot-spawn
/// loop loads this per connection_id and feeds it into the same shape
/// `run_contact_session_loop` expects.
#[derive(Debug, Clone)]
pub struct HandshakePersistedFields {
    pub rcv_id: Vec<u8>,
    pub rcv_dh_private: [u8; 32],
    pub srv_dh_public: [u8; 32],
    pub peer_queue: crate::protocol::smp_queue_info::SmpQueueInfo,
}

/// Briefing 044g.2: row shape returned by `list_subscribable_connections`.
/// Carries the keys 044g.2's boot-spawn worker needs to identify a
/// connection plus the server endpoint / fingerprint required to call
/// `establish_smp_session`.
#[derive(Debug, Clone)]
pub struct SubscribableConnection {
    pub connection_id: i64,
    pub contact_id: String,
    pub server_host: String,
    pub server_port: u16,
    pub fingerprint: String,
}

/// Briefing 044g.1a-fix1: maps legacy free-form contact-status strings
/// to the canonical connections.conn_status vocabulary defined in
/// 044g.1a. Without normalisation, values like "pending_hello" land
/// directly in the column and get classified as `ELSE 9` by
/// resolve_active_connection_id's CASE, breaking 044g.2's status-aware
/// boot-spawn filter.
///
/// Legacy values come from set_contact_status() calls predating the
/// connections-table refactor (see service.rs:1224, 2059).
/// Already-canonical values pass through unchanged. Unknown values
/// default to "New" with a warn log.
pub(crate) fn normalize_legacy_status(legacy: &str) -> &'static str {
    match legacy {
        "pending"       => "New",
        "pending_hello" => "Secured",
        "connected"     => "Active",
        "New"           => "New",
        "Confirmed"     => "Confirmed",
        "Secured"       => "Secured",
        "Active"        => "Active",
        "Disabled"      => "Disabled",
        unknown => {
            tracing::warn!(
                value = unknown,
                "normalize_legacy_status: unknown legacy value, defaulting to 'New'"
            );
            "New"
        }
    }
}

/// The singleton local user profile persisted in `user_profile`.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)] // created_at/updated_at exposed for future profile diagnostics
pub struct UserProfile {
    pub display_name: String,
    pub full_name: String,
    pub bio: String,
    /// Serialized preferences blob (JSON). Kept as a string so the profile
    /// persistence layer stays agnostic of the preferences schema.
    pub preferences_json: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A row from the contacts table.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields used in Phase 2
pub struct ContactRow {
    pub id: String,
    pub display_name: Option<String>,
    pub server_host: Option<String>,
    pub status: String,
    pub created_at: i64,
}

/// Rich contact summary for Briefing 045 `ListSimplexContacts`.
///
/// Strictly Tier::PublicMetadata as classified in the briefing; safe to
/// transmit to the frontend without extra encryption. `contact_id` is
/// nominally Tier::ProtectedMetadata but included so the frontend can
/// correlate subsequent RPCs (send, mark-read) back to the backend row.
#[derive(Debug, Clone)]
pub struct ContactSummaryRow {
    pub contact_id: String,
    pub display_name: Option<String>,
    pub full_name: Option<String>,
    pub established_at_unix: i64,
    pub last_message_at_unix: Option<i64>,
    pub unread_count: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_store() -> (tempfile::TempDir, QueueStore) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = QueueStore::open(dir.path()).expect("open store");
        // Insert a contact so the FK on ratchet_states is satisfiable.
        store
            .save_contact(
                "test-contact-id",
                Some("Test"),
                "smp.example.com",
                5223,
                "fingerprint",
                "queue-id",
                "sender-key",
            )
            .expect("save_contact");
        (dir, store)
    }

    #[test]
    fn ratchet_state_save_and_load_roundtrip() {
        let (_dir, store) = fresh_store();

        let blob = vec![1, 2, 3, 4, 5];
        store
            .save_ratchet_state("test-contact-id", &blob, 1)
            .expect("save");

        let loaded = store.load_ratchet_state("test-contact-id").expect("load");
        assert_eq!(loaded, Some((blob, 1)));

        let missing = store.load_ratchet_state("nonexistent").expect("load");
        assert_eq!(missing, None);
    }

    #[test]
    fn ratchet_state_overwrite() {
        let (_dir, store) = fresh_store();

        store
            .save_ratchet_state("test-contact-id", &[1, 2, 3], 1)
            .expect("first save");
        store
            .save_ratchet_state("test-contact-id", &[4, 5, 6, 7], 1)
            .expect("second save");

        let loaded = store
            .load_ratchet_state("test-contact-id")
            .expect("load")
            .expect("row present");
        assert_eq!(loaded.0, vec![4, 5, 6, 7]);
        assert_eq!(loaded.1, 1);
    }

    #[test]
    fn ratchet_state_cascade_on_contact_delete() {
        let (_dir, store) = fresh_store();
        store
            .save_ratchet_state("test-contact-id", &[9, 9, 9], 1)
            .expect("save");

        // Direct delete on contacts to exercise the FK CASCADE path.
        // The full WipeAllSimplexContacts flow also DELETEs ratchet_states
        // explicitly; this test covers the schema-level guarantee
        // independently of that belt-and-braces wipe.
        {
            let conn = store.conn.lock().unwrap();
            conn.execute("DELETE FROM contacts WHERE id = ?1", ["test-contact-id"])
                .expect("delete contact");
        }

        let loaded = store.load_ratchet_state("test-contact-id").expect("load");
        assert_eq!(loaded, None, "ratchet row should cascade with contact");
    }

    // -------- Briefing 044e: rcv_auth_private --------

    #[test]
    fn rcv_auth_private_save_and_load_roundtrip() {
        let (_dir, store) = fresh_store();

        let seed = [42u8; 32];
        store
            .save_rcv_auth_private("test-contact-id", &seed)
            .expect("save");

        let loaded = store
            .load_rcv_auth_private("test-contact-id")
            .expect("load");
        assert_eq!(loaded, Some(seed));

        let missing = store.load_rcv_auth_private("nonexistent").expect("load");
        assert_eq!(missing, None);
    }

    #[test]
    fn rcv_auth_private_overwrite() {
        let (_dir, store) = fresh_store();

        let first = [1u8; 32];
        let second = [2u8; 32];

        store
            .save_rcv_auth_private("test-contact-id", &first)
            .expect("first save");
        store
            .save_rcv_auth_private("test-contact-id", &second)
            .expect("second save");

        let loaded = store
            .load_rcv_auth_private("test-contact-id")
            .expect("load");
        assert_eq!(loaded, Some(second));
    }

    #[test]
    fn rcv_auth_private_returns_none_for_unset_column() {
        // fresh_store inserts a contact via save_contact (Briefing 044c
        // path) which never touches the rcv_auth_private column. The
        // BLOB stays NULL until 044e's save runs.
        let (_dir, store) = fresh_store();

        let loaded = store
            .load_rcv_auth_private("test-contact-id")
            .expect("load");
        assert_eq!(loaded, None, "unset column should return None");
    }

    // -------- Briefing 044g.1a-fix1: normalize_legacy_status --------

    #[test]
    fn normalize_legacy_status_maps_legacy_values() {
        assert_eq!(normalize_legacy_status("pending"), "New");
        assert_eq!(normalize_legacy_status("pending_hello"), "Secured");
        assert_eq!(normalize_legacy_status("connected"), "Active");
    }

    #[test]
    fn normalize_legacy_status_passes_canonical_values() {
        assert_eq!(normalize_legacy_status("New"), "New");
        assert_eq!(normalize_legacy_status("Confirmed"), "Confirmed");
        assert_eq!(normalize_legacy_status("Secured"), "Secured");
        assert_eq!(normalize_legacy_status("Active"), "Active");
        assert_eq!(normalize_legacy_status("Disabled"), "Disabled");
    }

    #[test]
    fn normalize_legacy_status_unknown_defaults_to_new() {
        // Unknown values default to "New" (the safest classification:
        // resolve_active_connection_id deprioritises it vs Active/Secured).
        assert_eq!(normalize_legacy_status("definitely-not-a-real-status"), "New");
        assert_eq!(normalize_legacy_status(""), "New");
    }

    // -------- Briefing 044g.2: list_subscribable_connections --------

    #[test]
    fn list_subscribable_connections_includes_fully_populated_row() {
        let (_dir, store) = fresh_store();
        let connection_id = store
            .resolve_active_connection_id("test-contact-id")
            .expect("resolve")
            .expect("connection exists");

        // Populate all required fields. fresh_store inserted via save_contact
        // (writes server_host/port/fingerprint/queue_id) and conn_status='New'.
        // Need to fill the four 044g.1b fields plus queue_auth + rcv_auth.
        store
            .save_handshake_persistence_fields(
                connection_id,
                &[0xab; 24],
                &[0xcd; 32],
                &[0xef; 32],
                &fixture_peer_queue(),
            )
            .expect("save handshake fields");
        store
            .save_queue_auth_private_for_connection(connection_id, &[0x01; 32])
            .expect("save queue_auth");
        store
            .save_rcv_auth_private_for_connection(connection_id, &[0x02; 32])
            .expect("save rcv_auth");

        let subscribable = store
            .list_subscribable_connections()
            .expect("list subscribable");
        assert_eq!(subscribable.len(), 1, "fully populated row should appear");
        let row = &subscribable[0];
        assert_eq!(row.connection_id, connection_id);
        assert_eq!(row.contact_id, "test-contact-id");
        assert_eq!(row.server_host, "smp.example.com");
        assert_eq!(row.server_port, 5223);
        assert_eq!(row.fingerprint, "fingerprint");
    }

    #[test]
    fn list_subscribable_connections_excludes_legacy_row_with_null_fields() {
        // fresh_store creates a contact + connection but does not call
        // save_handshake_persistence_fields - the four fields stay NULL.
        // The migration's UPDATE clause set to_subscribe=0 only for rows
        // that EXISTED at migration time; the freshly-inserted row from
        // save_contact has to_subscribe=1 (column default). Even so, the
        // SQL filter excludes it because rcv_id IS NULL.
        let (_dir, store) = fresh_store();

        let subscribable = store
            .list_subscribable_connections()
            .expect("list subscribable");
        assert!(
            subscribable.is_empty(),
            "row with NULL persistence fields must be excluded"
        );
    }

    #[test]
    fn list_subscribable_connections_excludes_disabled_status() {
        let (_dir, store) = fresh_store();
        let connection_id = store
            .resolve_active_connection_id("test-contact-id")
            .expect("resolve")
            .expect("connection exists");

        // Populate everything that would otherwise qualify the row.
        store
            .save_handshake_persistence_fields(
                connection_id,
                &[0xab; 24],
                &[0xcd; 32],
                &[0xef; 32],
                &fixture_peer_queue(),
            )
            .expect("save handshake fields");
        store
            .save_queue_auth_private_for_connection(connection_id, &[0x01; 32])
            .expect("save queue_auth");
        store
            .save_rcv_auth_private_for_connection(connection_id, &[0x02; 32])
            .expect("save rcv_auth");

        // Now disable. resolve_active_connection_id excludes Disabled too,
        // so set the status directly via the connection-keyed helper to
        // avoid the resolve dependency loop.
        store
            .set_connection_status_for_connection(connection_id, "Disabled")
            .expect("set status");

        let subscribable = store
            .list_subscribable_connections()
            .expect("list subscribable");
        assert!(
            subscribable.is_empty(),
            "Disabled connection must be excluded"
        );
    }

    // -------- Briefing 044g.1b: handshake persistence fields --------

    fn fixture_peer_queue() -> crate::protocol::smp_queue_info::SmpQueueInfo {
        crate::protocol::smp_queue_info::SmpQueueInfo {
            smp_client_version: 4,
            server_host: "smp.peer.example".to_string(),
            extra_hosts: vec![],
            server_port: "5223".to_string(),
            server_fingerprint: [0xab; 32],
            queue_id: [0x12; 24],
            sender_dh_public: [0xcd; 32],
            queue_mode: Some('M'),
        }
    }

    #[test]
    fn save_handshake_persistence_fields_roundtrip() {
        let (_dir, store) = fresh_store();
        let connection_id = store
            .resolve_active_connection_id("test-contact-id")
            .expect("resolve")
            .expect("connection exists from fresh_store");

        let rcv_id = vec![0x55; 24];
        let rcv_dh_private = [0x66; 32];
        let srv_dh_public = [0x77; 32];
        let peer_queue = fixture_peer_queue();

        store
            .save_handshake_persistence_fields(
                connection_id,
                &rcv_id,
                &rcv_dh_private,
                &srv_dh_public,
                &peer_queue,
            )
            .expect("save_handshake_persistence_fields");

        let loaded = store
            .load_handshake_persistence_fields(connection_id)
            .expect("load")
            .expect("fields present after save");
        assert_eq!(loaded.rcv_id, rcv_id);
        assert_eq!(loaded.rcv_dh_private, rcv_dh_private);
        assert_eq!(loaded.srv_dh_public, srv_dh_public);
        assert_eq!(loaded.peer_queue, peer_queue);

        // to_subscribe should have been set to 1 by the atomic save
        let to_sub: i64 = {
            let conn = store.conn.lock().unwrap();
            conn.query_row(
                "SELECT to_subscribe FROM connections WHERE connection_id = ?1",
                rusqlite::params![connection_id],
                |row| row.get(0),
            )
            .expect("select to_subscribe")
        };
        assert_eq!(to_sub, 1, "save_handshake_persistence_fields must set to_subscribe=1");
    }

    #[test]
    fn load_handshake_persistence_fields_returns_none_when_missing() {
        // fresh_store creates a contact + connection but never saves the
        // handshake fields. Loader must return Ok(None) without error.
        let (_dir, store) = fresh_store();
        let connection_id = store
            .resolve_active_connection_id("test-contact-id")
            .expect("resolve")
            .expect("connection exists from fresh_store");

        let loaded = store
            .load_handshake_persistence_fields(connection_id)
            .expect("load");
        assert!(loaded.is_none(), "missing fields must return None, not Err");
    }

    // -------- Briefing 044g.1a-fix1: update_contact_profile --------

    #[test]
    fn update_contact_profile_writes_identity_fields() {
        let (_dir, store) = fresh_store();
        store
            .update_contact_profile(
                "test-contact-id",
                "Alice Real",
                "Alice von Wonderland",
                "Just curious",
            )
            .expect("update_contact_profile");

        let conn = store.conn.lock().unwrap();
        let (dn, fnm, bio): (String, String, String) = conn
            .query_row(
                "SELECT display_name, full_name, bio FROM contacts WHERE id = ?1",
                rusqlite::params!["test-contact-id"],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("select identity row");
        assert_eq!(dn, "Alice Real");
        assert_eq!(fnm, "Alice von Wonderland");
        assert_eq!(bio, "Just curious");
    }
}
