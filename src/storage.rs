use rusqlite::{Connection, Result};
use crate::models::User;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::User;

    fn setup_in_memory_db() -> Storage {
        Storage::new(":memory:").unwrap()
    }

    #[test]
    fn test_save_and_get_user() {
        let storage = setup_in_memory_db();
        let user = User {
            id: Uuid::new_v4(),
            username: "alice".to_string(),
            public_key: "dummy_public_key".to_string(),
        };

        storage.save_user(&user).unwrap();
        let retrieved = storage.get_user_by_username("alice").unwrap();
        assert_eq!(user.id, retrieved.id);
        assert_eq!(user.username, retrieved.username);
        assert_eq!(user.public_key, retrieved.public_key);
    }

    #[test]
    fn test_save_duplicate_username() {
        let storage = setup_in_memory_db();
        let user1 = User {
            id: Uuid::new_v4(),
            username: "bob".to_string(),
            public_key: "key1".to_string(),
        };
        let user2 = User {
            id: Uuid::new_v4(),
            username: "bob".to_string(),
            public_key: "key2".to_string(),
        };

        storage.save_user(&user1).unwrap();
        let result = storage.save_user(&user2);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_nonexistent_user() {
        let storage = setup_in_memory_db();
        let result = storage.get_user_by_username("nonexistent");
        assert!(result.is_err());
    }
}