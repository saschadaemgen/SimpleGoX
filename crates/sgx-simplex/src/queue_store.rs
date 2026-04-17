//! SQLite persistence for SimpleX contacts, queues, and messages.

use anyhow::Result;
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

    CREATE TABLE IF NOT EXISTS ratchet_states (
        contact_id      TEXT PRIMARY KEY,
        root_key        BLOB NOT NULL,
        chain_key_send  BLOB NOT NULL,
        chain_key_recv  BLOB NOT NULL,
        hk_send         BLOB NOT NULL,
        hk_recv         BLOB NOT NULL,
        nhk_send        BLOB NOT NULL,
        nhk_recv        BLOB NOT NULL,
        dh_self_private BLOB NOT NULL,
        dh_self_public  BLOB NOT NULL,
        dh_peer         BLOB NOT NULL,
        msg_num_send    INTEGER NOT NULL DEFAULT 0,
        msg_num_recv    INTEGER NOT NULL DEFAULT 0,
        prev_chain_len  INTEGER NOT NULL DEFAULT 0,
        assoc_data      BLOB NOT NULL,
        updated_at      INTEGER NOT NULL DEFAULT (unixepoch())
    );

    CREATE TABLE IF NOT EXISTS sender_auth (
        contact_id  TEXT PRIMARY KEY,
        private_key BLOB NOT NULL,
        public_key  BLOB NOT NULL
    );
"#;

/// Wraps a SQLite connection for SimpleX data persistence.
pub struct QueueStore {
    conn: Mutex<Connection>,
}

impl QueueStore {
    /// Open or create the SQLite database and run migrations.
    pub fn open(data_dir: &Path) -> Result<Self> {
        let db_path = data_dir.join("simplex.db");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(SCHEMA)?;
        tracing::info!("SimpleX store opened at {:?}", db_path);
        Ok(Self {
            conn: Mutex::new(conn),
        })
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
        let name = stmt
            .query_row([], |row| row.get::<_, String>(0))
            .ok();
        Ok(name)
    }

    /// Save a new contact from a parsed invitation link.
    pub fn save_contact(
        &self,
        id: &str,
        display_name: Option<&str>,
        server_host: &str,
        server_port: u16,
        fingerprint: &str,
        queue_id: &str,
        sender_key: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO contacts (id, display_name, server_host, server_port, fingerprint, queue_id, sender_key)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![id, display_name, server_host, server_port as i64, fingerprint, queue_id, sender_key],
        )?;
        Ok(())
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
        conn.execute_batch(
            "DELETE FROM messages; DELETE FROM contacts; DELETE FROM profile;",
        )?;
        Ok(())
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

    /// Save queue IDs and auth keys from handshake Steps 2-4.
    #[allow(dead_code)]
    pub fn save_handshake_keys(
        &self,
        contact_id: &str,
        rcv_id: &[u8],
        snd_id: &[u8],
        rcv_auth_private: &[u8],
        rcv_dh_private: &[u8],
        rcv_dh_public: &[u8],
        snd_auth_private: &[u8],
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE contacts SET rcv_id=?2, snd_id=?3, rcv_auth_private=?4,
             rcv_dh_private=?5, rcv_dh_public=?6, snd_auth_private=?7
             WHERE id=?1",
            rusqlite::params![
                contact_id, rcv_id, snd_id, rcv_auth_private,
                rcv_dh_private, rcv_dh_public, snd_auth_private
            ],
        )?;
        Ok(())
    }

    /// Save X448 E2E keypairs generated for AgentInvitation.
    #[allow(dead_code)]
    pub fn save_e2e_keypairs(
        &self,
        contact_id: &str,
        key1_private: &[u8],
        key1_public: &[u8],
        key2_private: &[u8],
        key2_public: &[u8],
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE contacts SET e2e_key1_private=?2, e2e_key1_public=?3,
             e2e_key2_private=?4, e2e_key2_public=?5
             WHERE id=?1",
            rusqlite::params![contact_id, key1_private, key1_public, key2_private, key2_public],
        )?;
        Ok(())
    }

    /// Load saved X448 E2E keypairs for X3DH.
    #[allow(dead_code)]
    pub fn load_e2e_keypairs(&self, contact_id: &str) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>)> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT e2e_key1_private, e2e_key1_public, e2e_key2_private, e2e_key2_public FROM contacts WHERE id=?1"
        )?;
        let row = stmt.query_row([contact_id], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Vec<u8>>(3)?,
            ))
        })?;
        Ok(row)
    }

    /// Update contact status.
    pub fn set_contact_status(&self, contact_id: &str, status: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE contacts SET status=?2 WHERE id=?1",
            rusqlite::params![contact_id, status],
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
