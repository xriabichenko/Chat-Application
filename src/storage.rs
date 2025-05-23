use rusqlite::{Connection, Result};
use crate::models::{Group, Message, User, File};
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
        conn.execute(
            "CREATE TABLE IF NOT EXISTS groups (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL
            )",
            [],
        )?;
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
        conn.execute(
            "CREATE TABLE IF NOT EXISTS files (
                id TEXT PRIMARY KEY,
                sender_id TEXT NOT NULL,
                receiver_id TEXT,
                group_id TEXT,
                filename TEXT NOT NULL,
                data TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                FOREIGN KEY (sender_id) REFERENCES users(id),
                FOREIGN KEY (receiver_id) REFERENCES users(id),
                FOREIGN KEY (group_id) REFERENCES groups(id)
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

    pub fn get_all_users(&self) -> Result<Vec<User>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, username, public_key FROM users",
        )?;
        let user_iter = stmt.query_map([], |row| {
            Ok(User {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                username: row.get(1)?,
                public_key: row.get(2)?,
            })
        })?;
        let users = user_iter.collect::<Result<Vec<_>>>()?;
        Ok(users)
    }

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

    pub fn get_messages_between_users(&self, user1: Uuid, user2: Uuid) -> Result<Vec<Message>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, sender_id, receiver_id, group_id, content, timestamp
             FROM messages
             WHERE (sender_id = ?1 AND receiver_id = ?2)
                OR (sender_id = ?2 AND receiver_id = ?1)
             ORDER BY timestamp",
        )?;
        let message_iter = stmt.query_map(
            [&user1.to_string(), &user2.to_string()],
            |row| {
                Ok(Message {
                    id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                    sender_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap(),
                    receiver_id: row.get::<_, Option<String>>(2)?
                        .map(|s| Uuid::parse_str(&s).unwrap()),
                    group_id: row.get::<_, Option<String>>(3)?
                        .map(|s| Uuid::parse_str(&s).unwrap()),
                    content: row.get(4)?,
                    timestamp: row.get(5)?,
                })
            },
        )?;
        let messages = message_iter.collect::<Result<Vec<_>>>()?;
        Ok(messages)
    }

    pub fn get_messages_by_group_id(&self, group_id: Uuid) -> Result<Vec<Message>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, sender_id, receiver_id, group_id, content, timestamp
             FROM messages
             WHERE group_id = ?1
             ORDER BY timestamp",
        )?;
        let message_iter = stmt.query_map([&group_id.to_string()], |row| {
            Ok(Message {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                sender_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap(),
                receiver_id: row.get::<_, Option<String>>(2)?
                    .map(|s| Uuid::parse_str(&s).unwrap()),
                group_id: row.get::<_, Option<String>>(3)?
                    .map(|s| Uuid::parse_str(&s).unwrap()),
                content: row.get(4)?,
                timestamp: row.get(5)?,
            })
        })?;
        let messages = message_iter.collect::<Result<Vec<_>>>()?;
        Ok(messages)
    }

    pub fn save_group(&self, group: &Group) -> Result<()> {
        self.conn.execute(
            "INSERT INTO groups (id, name) VALUES (?1, ?2)",
            (&group.id.to_string(), &group.name),
        )?;
        for user_id in &group.members {
            self.conn.execute(
                "INSERT INTO group_members (group_id, user_id) VALUES (?1, ?2)",
                (&group.id.to_string(), &user_id.to_string()),
            )?;
        }
        Ok(())
    }

    pub fn get_group_by_id(&self, group_id: &Uuid) -> Result<Group> {
        let mut stmt = self.conn.prepare("SELECT id, name FROM groups WHERE id = ?1")?;
        let group_info = stmt.query_row([&group_id.to_string()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
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

    pub fn get_groups_by_user_id(&self, user_id: Uuid) -> Result<Vec<Group>> {
        let mut stmt = self.conn.prepare(
            "SELECT g.id, g.name
             FROM groups g
             JOIN group_members gm ON g.id = gm.group_id
             WHERE gm.user_id = ?1",
        )?;
        let group_iter = stmt.query_map([&user_id.to_string()], |row| {
            let group_id = Uuid::parse_str(&row.get::<_, String>(0)?).unwrap();
            Ok((group_id, row.get::<_, String>(1)?))
        })?;
        let mut groups = Vec::new();
        for group_info in group_iter {
            let (group_id, name) = group_info?;
            let mut stmt = self.conn.prepare(
                "SELECT user_id FROM group_members WHERE group_id = ?1",
            )?;
            let member_rows = stmt.query_map([&group_id.to_string()], |row| {
                Ok(Uuid::parse_str(&row.get::<_, String>(0)?).unwrap())
            })?;
            let members: Vec<Uuid> = member_rows.collect::<Result<Vec<_>>>()?;
            groups.push(Group {
                id: group_id,
                name,
                members,
            });
        }
        Ok(groups)
    }

    pub fn save_file(&self, file: &File) -> Result<()> {
        self.conn.execute(
            "INSERT INTO files (id, sender_id, receiver_id, group_id, filename, data, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            (
                &file.id.to_string(),
                &file.sender_id.to_string(),
                &file.receiver_id.map(|id| id.to_string()),
                &file.group_id.map(|id| id.to_string()),
                &file.filename,
                &file.data,
                &file.timestamp,
            ),
        )?;
        Ok(())
    }

    pub fn add_user_to_group(&self, group_id: &Uuid, user_id: &Uuid) -> Result<()> {
        self.conn.execute(
            "INSERT INTO group_members (group_id, user_id) VALUES (?1, ?2)",
            (&group_id.to_string(), &user_id.to_string()),
        )?;
        Ok(())
    }

    pub fn remove_user_from_group(&self, group_id: &Uuid, user_id: &Uuid) -> Result<()> {
        self.conn.execute(
            "DELETE FROM group_members WHERE group_id = ?1 AND user_id = ?2",
            (&group_id.to_string(), &user_id.to_string()),
        )?;
        Ok(())
    }

    pub fn delete_group(&self, group_id: &Uuid) -> Result<()> {
        // Delete group members
        self.conn.execute(
            "DELETE FROM group_members WHERE group_id = ?1",
            [&group_id.to_string()],
        )?;
        // Delete messages associated with the group
        self.conn.execute(
            "DELETE FROM messages WHERE group_id = ?1",
            [&group_id.to_string()],
        )?;
        // Delete files associated with the group
        self.conn.execute(
            "DELETE FROM files WHERE group_id = ?1",
            [&group_id.to_string()],
        )?;
        // Delete the group itself
        self.conn.execute(
            "DELETE FROM groups WHERE id = ?1",
            [&group_id.to_string()],
        )?;
        Ok(())
    }
}