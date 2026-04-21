use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension};

use crate::app::ReadingQueueItem;
use crate::permissions::{Capability, PermissionDecision};

#[derive(Debug, Clone)]
pub struct ProfileDb {
    path: PathBuf,
}

impl ProfileDb {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, String> {
        let db = Self { path: path.into() };
        db.init()?;
        Ok(db)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn connect(&self) -> Result<Connection, String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create db dir failed: {e}"))?;
        }
        Connection::open(&self.path).map_err(|e| format!("open sqlite failed: {e}"))
    }

    fn init(&self) -> Result<(), String> {
        let mut conn = self.connect()?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| format!("pragma journal_mode failed: {e}"))?;

        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS schema_meta (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL
            );
            INSERT OR IGNORE INTO schema_meta(key,value) VALUES('version','1');

            CREATE TABLE IF NOT EXISTS history (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              url TEXT NOT NULL,
              title TEXT,
              visited_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS tts_state (
              id INTEGER PRIMARY KEY CHECK(id=1),
              playing INTEGER NOT NULL,
              queue_json TEXT NOT NULL
            );
            INSERT OR IGNORE INTO tts_state(id,playing,queue_json) VALUES(1,0,'[]');

            CREATE TABLE IF NOT EXISTS reading_queue (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              url TEXT NOT NULL UNIQUE,
              title TEXT,
              kind TEXT NOT NULL,
              saved_at TEXT NOT NULL,
              progress REAL NOT NULL,
              article_text TEXT
            );

            CREATE TABLE IF NOT EXISTS visit_stats (
              id INTEGER PRIMARY KEY CHECK(id=1),
              visit_total INTEGER NOT NULL,
              revisit_total INTEGER NOT NULL
            );
            INSERT OR IGNORE INTO visit_stats(id,visit_total,revisit_total) VALUES(1,0,0);

            CREATE TABLE IF NOT EXISTS visit_counts (
              url TEXT PRIMARY KEY,
              count INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS permission_grants (
              domain TEXT NOT NULL,
              capability TEXT NOT NULL,
              decision TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              PRIMARY KEY(domain, capability)
            );
            "#,
        )
        .map_err(|e| format!("init schema failed: {e}"))?;
        Ok(())
    }

    pub fn append_history(&self, url: &str, title: Option<&str>, visited_at: &str) -> Result<(), String> {
        let mut conn = self.connect()?;
        conn.execute(
            "INSERT INTO history(url,title,visited_at) VALUES(?,?,?)",
            (url, title, visited_at),
        )
        .map_err(|e| format!("insert history failed: {e}"))?;
        Ok(())
    }

    pub fn load_history(&self, limit: usize) -> Result<Vec<String>, String> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare("SELECT url FROM history ORDER BY id DESC LIMIT ?")
            .map_err(|e| format!("prepare history failed: {e}"))?;
        let mut rows = stmt
            .query([limit as i64])
            .map_err(|e| format!("query history failed: {e}"))?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(|e| format!("read history row failed: {e}"))? {
            let url: String = row.get(0).map_err(|e| format!("get url failed: {e}"))?;
            out.push(url);
        }
        Ok(out)
    }

    pub fn save_tts_state(&self, playing: bool, queue: &[String]) -> Result<(), String> {
        let mut conn = self.connect()?;
        let json = serde_json::to_string(queue).map_err(|e| format!("serialize tts queue: {e}"))?;
        conn.execute(
            "UPDATE tts_state SET playing=?, queue_json=? WHERE id=1",
            (if playing { 1 } else { 0 }, json),
        )
        .map_err(|e| format!("update tts_state failed: {e}"))?;
        Ok(())
    }

    pub fn load_tts_state(&self) -> Result<(bool, Vec<String>), String> {
        let conn = self.connect()?;
        let row: Option<(i64, String)> = conn
            .query_row("SELECT playing, queue_json FROM tts_state WHERE id=1", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .optional()
            .map_err(|e| format!("load tts_state failed: {e}"))?;
        let Some((playing, json)) = row else {
            return Ok((false, Vec::new()));
        };
        let queue: Vec<String> =
            serde_json::from_str(&json).map_err(|e| format!("parse tts queue: {e}"))?;
        Ok((playing != 0, queue))
    }

    pub fn upsert_reading_item(&self, item: &ReadingQueueItem) -> Result<(), String> {
        let mut conn = self.connect()?;
        conn.execute(
            r#"
            INSERT INTO reading_queue(url,title,kind,saved_at,progress,article_text)
            VALUES(?,?,?,?,?,?)
            ON CONFLICT(url) DO UPDATE SET
              title=excluded.title,
              kind=excluded.kind,
              saved_at=excluded.saved_at,
              progress=excluded.progress,
              article_text=excluded.article_text
            "#,
            (
                &item.url,
                item.title.as_deref(),
                &item.kind,
                &item.saved_at,
                item.progress,
                item.article_text.as_deref(),
            ),
        )
        .map_err(|e| format!("upsert reading item failed: {e}"))?;
        Ok(())
    }

    pub fn remove_reading_item(&self, url: &str) -> Result<(), String> {
        let mut conn = self.connect()?;
        conn.execute("DELETE FROM reading_queue WHERE url=?", [url])
            .map_err(|e| format!("delete reading item failed: {e}"))?;
        Ok(())
    }

    pub fn clear_reading_queue(&self) -> Result<(), String> {
        let mut conn = self.connect()?;
        conn.execute("DELETE FROM reading_queue", [])
            .map_err(|e| format!("clear reading queue failed: {e}"))?;
        Ok(())
    }

    pub fn load_reading_queue(&self, limit: usize) -> Result<Vec<ReadingQueueItem>, String> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare(
                r#"
                SELECT url,title,kind,saved_at,progress,article_text
                FROM reading_queue
                ORDER BY id DESC
                LIMIT ?
                "#,
            )
            .map_err(|e| format!("prepare reading queue failed: {e}"))?;
        let mut rows = stmt
            .query([limit as i64])
            .map_err(|e| format!("query reading queue failed: {e}"))?;
        let mut out = Vec::new();
        while let Some(row) = rows
            .next()
            .map_err(|e| format!("read reading queue row failed: {e}"))?
        {
            out.push(ReadingQueueItem {
                url: row.get(0).map_err(|e| format!("get url failed: {e}"))?,
                title: row.get(1).map_err(|e| format!("get title failed: {e}"))?,
                kind: row.get(2).map_err(|e| format!("get kind failed: {e}"))?,
                saved_at: row.get(3).map_err(|e| format!("get saved_at failed: {e}"))?,
                progress: row.get(4).map_err(|e| format!("get progress failed: {e}"))?,
                article_text: row.get(5).map_err(|e| format!("get article_text failed: {e}"))?,
            });
        }
        Ok(out)
    }

    pub fn save_visit_stats(
        &self,
        visit_total: u64,
        revisit_total: u64,
        visit_counts: &std::collections::HashMap<String, u32>,
    ) -> Result<(), String> {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction()
            .map_err(|e| format!("begin tx failed: {e}"))?;
        tx.execute(
            "UPDATE visit_stats SET visit_total=?, revisit_total=? WHERE id=1",
            (visit_total as i64, revisit_total as i64),
        )
        .map_err(|e| format!("update visit_stats failed: {e}"))?;
        tx.execute("DELETE FROM visit_counts", [])
            .map_err(|e| format!("clear visit_counts failed: {e}"))?;
        {
            let mut stmt = tx
                .prepare("INSERT INTO visit_counts(url,count) VALUES(?,?)")
                .map_err(|e| format!("prepare visit_counts insert failed: {e}"))?;
            for (url, count) in visit_counts {
                stmt.execute((url, *count as i64))
                    .map_err(|e| format!("insert visit_count failed: {e}"))?;
            }
        }
        tx.commit().map_err(|e| format!("commit failed: {e}"))?;
        Ok(())
    }

    pub fn load_visit_stats(
        &self,
    ) -> Result<(u64, u64, std::collections::HashMap<String, u32>), String> {
        let conn = self.connect()?;
        let (visit_total, revisit_total): (i64, i64) = conn
            .query_row(
                "SELECT visit_total,revisit_total FROM visit_stats WHERE id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| format!("load visit_stats failed: {e}"))?;

        let mut counts = std::collections::HashMap::new();
        let mut stmt = conn
            .prepare("SELECT url,count FROM visit_counts")
            .map_err(|e| format!("prepare visit_counts load failed: {e}"))?;
        let mut rows = stmt
            .query([])
            .map_err(|e| format!("query visit_counts failed: {e}"))?;
        while let Some(row) = rows
            .next()
            .map_err(|e| format!("read visit_counts row failed: {e}"))?
        {
            let url: String = row.get(0).map_err(|e| format!("get url failed: {e}"))?;
            let count: i64 = row.get(1).map_err(|e| format!("get count failed: {e}"))?;
            counts.insert(url, count.max(0) as u32);
        }

        Ok((visit_total.max(0) as u64, revisit_total.max(0) as u64, counts))
    }

    pub fn upsert_permission_grant(
        &self,
        domain: &str,
        capability: Capability,
        decision: PermissionDecision,
        updated_at: &str,
    ) -> Result<(), String> {
        let conn = self.connect()?;
        conn.execute(
            r#"
            INSERT INTO permission_grants(domain, capability, decision, updated_at)
            VALUES(?,?,?,?)
            ON CONFLICT(domain, capability) DO UPDATE SET
              decision=excluded.decision,
              updated_at=excluded.updated_at
            "#,
            (domain, capability.label(), decision.label(), updated_at),
        )
        .map_err(|e| format!("upsert permission grant failed: {e}"))?;
        Ok(())
    }

    pub fn load_permission_grants(
        &self,
    ) -> Result<std::collections::BTreeMap<String, std::collections::BTreeMap<Capability, PermissionDecision>>, String>
    {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare("SELECT domain, capability, decision FROM permission_grants")
            .map_err(|e| format!("prepare permission grants failed: {e}"))?;
        let mut rows = stmt
            .query([])
            .map_err(|e| format!("query permission grants failed: {e}"))?;
        let mut out: std::collections::BTreeMap<
            String,
            std::collections::BTreeMap<Capability, PermissionDecision>,
        > = std::collections::BTreeMap::new();
        while let Some(row) = rows
            .next()
            .map_err(|e| format!("read permission grant row failed: {e}"))?
        {
            let domain: String = row.get(0).map_err(|e| format!("get domain failed: {e}"))?;
            let capability: String =
                row.get(1).map_err(|e| format!("get capability failed: {e}"))?;
            let decision: String =
                row.get(2).map_err(|e| format!("get decision failed: {e}"))?;
            let Some(capability) = Capability::from_label(&capability) else {
                continue;
            };
            let Some(decision) = PermissionDecision::from_label(&decision) else {
                continue;
            };
            out.entry(domain)
                .or_default()
                .insert(capability, decision);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_round_trips_reading_queue_and_tts() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.sqlite");
        let db = ProfileDb::open(&db_path).unwrap();

        db.save_tts_state(true, &["a".to_string(), "b".to_string()])
            .unwrap();
        let (playing, queue) = db.load_tts_state().unwrap();
        assert!(playing);
        assert_eq!(queue, vec!["a".to_string(), "b".to_string()]);

        let item = ReadingQueueItem {
            url: "https://example.com".to_string(),
            title: Some("Example".to_string()),
            kind: "article".to_string(),
            saved_at: "now".to_string(),
            progress: 0.5,
            article_text: Some("hello".to_string()),
        };
        db.upsert_reading_item(&item).unwrap();
        let loaded = db.load_reading_queue(10).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].url, item.url);
        assert_eq!(loaded[0].article_text.as_deref(), Some("hello"));
    }

    #[test]
    fn db_round_trips_visit_stats() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.sqlite");
        let db = ProfileDb::open(&db_path).unwrap();
        let mut counts = std::collections::HashMap::new();
        counts.insert("u1".to_string(), 2);
        counts.insert("u2".to_string(), 1);
        db.save_visit_stats(3, 1, &counts).unwrap();
        let (visit_total, revisit_total, loaded) = db.load_visit_stats().unwrap();
        assert_eq!(visit_total, 3);
        assert_eq!(revisit_total, 1);
        assert_eq!(loaded.get("u1").copied(), Some(2));
    }

    #[test]
    fn db_round_trips_permission_grants() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.sqlite");
        let db = ProfileDb::open(&db_path).unwrap();
        db.upsert_permission_grant(
            "example.com",
            Capability::TerminalExec,
            PermissionDecision::Allow,
            "now",
        )
        .unwrap();
        let loaded = db.load_permission_grants().unwrap();
        assert_eq!(
            loaded
                .get("example.com")
                .and_then(|m| m.get(&Capability::TerminalExec))
                .copied(),
            Some(PermissionDecision::Allow)
        );
    }
}
