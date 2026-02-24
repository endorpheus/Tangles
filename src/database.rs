use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: Option<i64>,
    pub title: String,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
    pub position_x: f64,
    pub position_y: f64,
    pub is_visible: bool,
    pub always_on_top: bool,
    pub width: i32,
    pub height: i32,
    pub theme_bg: Option<String>,
    pub theme_fg: Option<String>,
    pub theme_accent: Option<String>,
    pub custom_colors: Option<String>,
    pub chromeless: bool,
    pub star_color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub id: Option<i64>,
    pub source_note_id: i64,
    pub target_note_id: i64,
    pub link_type: LinkType,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LinkType {
    Connection,     // Visual mind-map connection
    WordReference,  // Word highlighting link
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordIndex {
    pub id: Option<i64>,
    pub word: String,
    pub note_id: i64,
    pub frequency: i32, // How many times word appears in note
}

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn new(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        // Performance pragmas
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA cache_size=-8000;
             PRAGMA temp_store=MEMORY;
             PRAGMA mmap_size=268435456;"
        )?;
        let db = Database { conn: Arc::new(Mutex::new(conn)) };
        db.init_tables()?;
        db.run_migrations()?;
        Ok(db)
    }

    fn init_tables(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS notes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                position_x REAL DEFAULT 0.0,
                position_y REAL DEFAULT 0.0,
                is_visible BOOLEAN DEFAULT 1,
                always_on_top BOOLEAN DEFAULT 0,
                width INTEGER DEFAULT 400,
                height INTEGER DEFAULT 300
            );
            CREATE TABLE IF NOT EXISTS links (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_note_id INTEGER NOT NULL,
                target_note_id INTEGER NOT NULL,
                link_type TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (source_note_id) REFERENCES notes (id) ON DELETE CASCADE,
                FOREIGN KEY (target_note_id) REFERENCES notes (id) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS word_index (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                word TEXT NOT NULL,
                note_id INTEGER NOT NULL,
                frequency INTEGER DEFAULT 1,
                FOREIGN KEY (note_id) REFERENCES notes (id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_word_index_word ON word_index(word);
            CREATE INDEX IF NOT EXISTS idx_word_index_note ON word_index(note_id);
            CREATE INDEX IF NOT EXISTS idx_links_source ON links(source_note_id);
            CREATE INDEX IF NOT EXISTS idx_links_target ON links(target_note_id);
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );"
        )?;
        Ok(())
    }

    fn run_migrations(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        // Add theme columns if they don't exist
        let has_theme_bg: bool = conn
            .prepare("SELECT theme_bg FROM notes LIMIT 0")
            .is_ok();
        if !has_theme_bg {
            conn.execute_batch(
                "ALTER TABLE notes ADD COLUMN theme_bg TEXT;
                 ALTER TABLE notes ADD COLUMN theme_fg TEXT;
                 ALTER TABLE notes ADD COLUMN theme_accent TEXT;"
            )?;
        }
        let has_custom_colors: bool = conn
            .prepare("SELECT custom_colors FROM notes LIMIT 0")
            .is_ok();
        if !has_custom_colors {
            conn.execute_batch("ALTER TABLE notes ADD COLUMN custom_colors TEXT;")?;
        }
        let has_chromeless: bool = conn
            .prepare("SELECT chromeless FROM notes LIMIT 0")
            .is_ok();
        if !has_chromeless {
            conn.execute_batch(
                "ALTER TABLE notes ADD COLUMN chromeless BOOLEAN DEFAULT 0;
                 ALTER TABLE notes ADD COLUMN star_color TEXT;"
            )?;
        }
        Ok(())
    }

    pub fn create_note(&self, note: &Note) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO notes (title, content, created_at, updated_at, position_x, position_y, is_visible, always_on_top, width, height, theme_bg, theme_fg, theme_accent, custom_colors, chromeless, star_color)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                note.title, note.content, note.created_at, note.updated_at,
                note.position_x, note.position_y, note.is_visible, note.always_on_top,
                note.width, note.height, note.theme_bg, note.theme_fg, note.theme_accent,
                note.custom_colors, note.chromeless, note.star_color
            ],
        )?;
        let note_id = conn.last_insert_rowid();
        Self::index_note_words_with_conn(&conn, note_id, &note.content)?;
        Ok(note_id)
    }

    pub fn update_note(&self, note: &Note) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE notes SET title = ?1, content = ?2, updated_at = ?3, position_x = ?4, position_y = ?5, is_visible = ?6, always_on_top = ?7, width = ?8, height = ?9, theme_bg = ?10, theme_fg = ?11, theme_accent = ?12, custom_colors = ?13, chromeless = ?14, star_color = ?15
             WHERE id = ?16",
            params![
                note.title, note.content, note.updated_at,
                note.position_x, note.position_y, note.is_visible, note.always_on_top,
                note.width, note.height, note.theme_bg, note.theme_fg, note.theme_accent,
                note.custom_colors, note.chromeless, note.star_color, note.id
            ],
        )?;
        if let Some(note_id) = note.id {
            conn.execute("DELETE FROM word_index WHERE note_id = ?1", [note_id])?;
            Self::index_note_words_with_conn(&conn, note_id, &note.content)?;
        }
        Ok(())
    }

    pub fn update_note_position(&self, id: i64, x: f64, y: f64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE notes SET position_x = ?1, position_y = ?2 WHERE id = ?3",
            params![x, y, id],
        )?;
        Ok(())
    }

    pub fn append_note_content(&self, id: i64, html: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE notes SET content = content || ?1 WHERE id = ?2",
            params![html, id],
        )?;
        Ok(())
    }

    pub fn get_note(&self, id: i64) -> Result<Option<Note>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT id, title, content, created_at, updated_at, position_x, position_y, is_visible, always_on_top, width, height, theme_bg, theme_fg, theme_accent, custom_colors, chromeless, star_color
             FROM notes WHERE id = ?1"
        )?;
        let mut rows = stmt.query_map([id], Self::row_to_note)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn get_all_notes(&self) -> Result<Vec<Note>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT id, title, content, created_at, updated_at, position_x, position_y, is_visible, always_on_top, width, height, theme_bg, theme_fg, theme_accent, custom_colors, chromeless, star_color
             FROM notes ORDER BY updated_at DESC"
        )?;
        let rows = stmt.query_map([], Self::row_to_note)?;
        rows.collect()
    }

    pub fn get_visible_notes(&self) -> Result<Vec<Note>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT id, title, content, created_at, updated_at, position_x, position_y, is_visible, always_on_top, width, height, theme_bg, theme_fg, theme_accent, custom_colors, chromeless, star_color
             FROM notes WHERE is_visible = 1"
        )?;
        let rows = stmt.query_map([], Self::row_to_note)?;
        rows.collect()
    }

    pub fn get_recent_notes(&self, limit: usize) -> Result<Vec<Note>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT id, title, content, created_at, updated_at, position_x, position_y, is_visible, always_on_top, width, height, theme_bg, theme_fg, theme_accent, custom_colors, chromeless, star_color
             FROM notes ORDER BY updated_at DESC LIMIT ?1"
        )?;
        let rows = stmt.query_map([limit as i64], Self::row_to_note)?;
        rows.collect()
    }

    pub fn get_note_by_title(&self, title: &str) -> Result<Option<Note>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT id, title, content, created_at, updated_at, position_x, position_y, is_visible, always_on_top, width, height, theme_bg, theme_fg, theme_accent, custom_colors, chromeless, star_color
             FROM notes WHERE title = ?1"
        )?;
        let mut rows = stmt.query_map([title], Self::row_to_note)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn get_notes_linking_to(&self, title: &str) -> Result<Vec<Note>> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("%tangle://{}%", title);
        let mut stmt = conn.prepare_cached(
            "SELECT id, title, content, created_at, updated_at, position_x, position_y, is_visible, always_on_top, width, height, theme_bg, theme_fg, theme_accent, custom_colors, chromeless, star_color
             FROM notes WHERE content LIKE ?1
             ORDER BY updated_at DESC"
        )?;
        let rows = stmt.query_map([pattern], Self::row_to_note)?;
        rows.collect()
    }

    pub fn get_all_note_titles(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached("SELECT title FROM notes ORDER BY title")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        rows.collect()
    }

    pub fn search_notes(&self, query: &str) -> Result<Vec<Note>> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("%{}%", query);
        let mut stmt = conn.prepare_cached(
            "SELECT id, title, content, created_at, updated_at, position_x, position_y, is_visible, always_on_top, width, height, theme_bg, theme_fg, theme_accent, custom_colors, chromeless, star_color
             FROM notes WHERE title LIKE ?1 OR content LIKE ?1
             ORDER BY updated_at DESC"
        )?;
        let rows = stmt.query_map([pattern], Self::row_to_note)?;
        rows.collect()
    }

    pub fn delete_note(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM notes WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn create_link(&self, link: &Link) -> Result<i64> {
        let link_type_str = match link.link_type {
            LinkType::Connection => "connection",
            LinkType::WordReference => "word_reference",
        };
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO links (source_note_id, target_note_id, link_type, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![link.source_note_id, link.target_note_id, link_type_str, link.created_at],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_links_for_note(&self, note_id: i64) -> Result<Vec<Link>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT id, source_note_id, target_note_id, link_type, created_at
             FROM links
             WHERE source_note_id = ?1 OR target_note_id = ?1"
        )?;
        let rows = stmt.query_map([note_id], |row| {
            let link_type_str: String = row.get(3)?;
            let link_type = match link_type_str.as_str() {
                "word_reference" => LinkType::WordReference,
                _ => LinkType::Connection,
            };
            Ok(Link {
                id: Some(row.get(0)?),
                source_note_id: row.get(1)?,
                target_note_id: row.get(2)?,
                link_type,
                created_at: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    pub fn find_notes_with_word(&self, word: &str) -> Result<Vec<Note>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT n.id, n.title, n.content, n.created_at, n.updated_at, n.position_x, n.position_y, n.is_visible, n.always_on_top, n.width, n.height, n.theme_bg, n.theme_fg, n.theme_accent, n.custom_colors, n.chromeless, n.star_color
             FROM notes n
             JOIN word_index w ON n.id = w.note_id
             WHERE w.word = ?1
             ORDER BY w.frequency DESC"
        )?;
        let rows = stmt.query_map([word.to_lowercase()], Self::row_to_note)?;
        rows.collect()
    }

    fn row_to_note(row: &rusqlite::Row) -> rusqlite::Result<Note> {
        Ok(Note {
            id: Some(row.get(0)?),
            title: row.get(1)?,
            content: row.get(2)?,
            created_at: row.get(3)?,
            updated_at: row.get(4)?,
            position_x: row.get(5)?,
            position_y: row.get(6)?,
            is_visible: row.get(7)?,
            always_on_top: row.get(8)?,
            width: row.get(9)?,
            height: row.get(10)?,
            theme_bg: row.get(11)?,
            theme_fg: row.get(12)?,
            theme_accent: row.get(13)?,
            custom_colors: row.get(14)?,
            chromeless: row.get(15)?,
            star_color: row.get(16)?,
        })
    }

    pub fn get_setting(&self, key: &str) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached("SELECT value FROM settings WHERE key = ?1").ok()?;
        stmt.query_row([key], |row| row.get(0)).ok()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    fn index_note_words_with_conn(conn: &Connection, note_id: i64, content: &str) -> Result<()> {
        use regex::Regex;
        use std::collections::HashMap;

        // Strip HTML tags before indexing
        let tag_re = Regex::new(r"<[^>]+>").unwrap();
        let plain = tag_re.replace_all(content, "");

        let word_regex = Regex::new(r"\b\w+\b").unwrap();
        let mut word_count: HashMap<String, i32> = HashMap::new();
        for cap in word_regex.find_iter(&plain) {
            let word = cap.as_str().to_lowercase();
            *word_count.entry(word).or_insert(0) += 1;
        }
        // Batch all inserts in a single transaction
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO word_index (word, note_id, frequency) VALUES (?1, ?2, ?3)"
            )?;
            for (word, frequency) in &word_count {
                stmt.execute(params![word, note_id, frequency])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}
