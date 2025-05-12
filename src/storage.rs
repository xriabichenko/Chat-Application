use rusqlite::{Connection, Result};
use crate::models::{Message, User};
use uuid::Uuid;

pub struct Storage {
    conn: Connection,
}

impl Storage {
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                username TEXT NOT NULL UNIQUE,
                public_key TEXT NOT NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                sender_id TEXT NOT NULL,
                group_id TEXT,
                content TEXT NOT NULL,
                timestamp INTEGER NOT NULL
            )",
            [],
        )?;
        Ok(Storage { conn })
    }

    pub fn save_user(&self, user: &User) -> Result<()> {
        self.conn.execute(
            "INSERT INTO users (id, username, public_key) VALUES (?1, ?2, ?3)",
            (&user.id.to_string(), &user.username, &user.public_key),
        )?;
        Ok(())
    }

    pub fn get_user_by_username(&self, username: &str) -> Result<User> {
        let mut stmt = self.conn.prepare("SELECT id, username, public_key FROM users WHERE username = ?1")?;
        let user = stmt.query_row([username], |row| {
            Ok(User {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                username: row.get(1)?,
                public_key: row.get(2)?,
            })
        })?;
        Ok(user)
    }

    pub fn save_message(&self, msg: &Message) -> Result<()> {
        self.conn.execute(
            "INSERT INTO messages (id, sender_id, group_id, content, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                &msg.id.to_string(),
                &msg.sender_id.to_string(),
                &msg.group_id.map(|id| id.to_string()),
                &msg.content,
                &msg.timestamp,
            ),
        )?;
        Ok(())
    }
}