use rusqlite::{Connection, Result};
use crate::models::{Group, Message, User};
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
                receiver_id TEXT,
                group_id TEXT,
                content TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                FOREIGN KEY (sender_id) REFERENCES users(id),
                FOREIGN KEY (receiver_id) REFERENCES users(id),
                FOREIGN KEY (group_id) REFERENCES groups(id)
            )",
            [],
        )?;

        // Таблица для групп
        conn.execute(
            "CREATE TABLE IF NOT EXISTS groups (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL
            )",
            [],
        )?;

        // Таблица для связи пользователей и групп (участники групп)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS group_members (
                group_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                PRIMARY KEY (group_id, user_id),
                FOREIGN KEY (group_id) REFERENCES groups(id),
                FOREIGN KEY (user_id) REFERENCES users(id)
            )",
            [],
        )?;

        Ok(Storage { conn })
    }

    /// Сохранение пользователя
    pub fn save_user(&self, user: &User) -> Result<()> {
        self.conn.execute(
            "INSERT INTO users (id, username, public_key) VALUES (?1, ?2, ?3)",
            (&user.id.to_string(), &user.username, &user.public_key),
        )?;
        Ok(())
    }

    /// Получение пользователя по имени
    pub fn get_user_by_username(&self, username: &str) -> Result<User> {
        let mut stmt = self.conn.prepare(
            "SELECT id, username, public_key FROM users WHERE username = ?1",
        )?;
        let user = stmt.query_row([username], |row| {
            Ok(User {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                username: row.get(1)?,
                public_key: row.get(2)?,
            })
        })?;
        Ok(user)
    }

    /// Сохранение сообщения
    pub fn save_message(&self, msg: &Message) -> Result<()> {
        self.conn.execute(
            "INSERT INTO messages (id, sender_id, receiver_id, group_id, content, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (
                &msg.id.to_string(),
                &msg.sender_id.to_string(),
                &msg.receiver_id.map(|id| id.to_string()),
                &msg.group_id.map(|id| id.to_string()),
                &msg.content,
                &msg.timestamp,
            ),
        )?;
        Ok(())
    }

    /// Сохранение группы и её участников
    pub fn save_group(&self, group: &Group) -> Result<()> {
        // Сохраняем группу
        self.conn.execute(
            "INSERT INTO groups (id, name) VALUES (?1, ?2)",
            (&group.id.to_string(), &group.name),
        )?;

        // Сохраняем участников группы
        for user_id in &group.members {
            self.conn.execute(
                "INSERT INTO group_members (group_id, user_id) VALUES (?1, ?2)",
                (&group.id.to_string(), &user_id.to_string()),
            )?;
        }

        Ok(())
    }

    /// Получение группы по её ID
    pub fn get_group_by_id(&self, group_id: &Uuid) -> Result<Group> {
        // Получаем информацию о группе
        let mut stmt = self.conn.prepare("SELECT id, name FROM groups WHERE id = ?1")?;
        let group_info = stmt.query_row([&group_id.to_string()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        // Получаем участников группы
        let mut stmt = self.conn.prepare(
            "SELECT user_id FROM group_members WHERE group_id = ?1",
        )?;
        let member_rows = stmt.query_map([&group_id.to_string()], |row| {
            Ok(Uuid::parse_str(&row.get::<_, String>(0)?).unwrap())
        })?;
        let members: Vec<Uuid> = member_rows.collect::<Result<Vec<_>>>()?;

        Ok(Group {
            id: Uuid::parse_str(&group_info.0).unwrap(),
            name: group_info.1,
            members,
        })
    }

    /// Добавление пользователя в группу
    pub fn add_user_to_group(&self, group_id: &Uuid, user_id: &Uuid) -> Result<()> {
        self.conn.execute(
            "INSERT INTO group_members (group_id, user_id) VALUES (?1, ?2)",
            (&group_id.to_string(), &user_id.to_string()),
        )?;
        Ok(())
    }
}