use serde::{Serialize, Deserialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub public_key: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Message {
    pub id: Uuid,
    pub sender_id: Uuid,
    pub receiver_id: Option<Uuid>,
    pub group_id: Option<Uuid>,
    pub content: String,
    pub timestamp: i64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    pub members: Vec<Uuid>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct File {
    pub id: Uuid,
    pub sender_id: Uuid,
    pub receiver_id: Option<Uuid>,
    pub group_id: Option<Uuid>,
    pub filename: String,
    pub data: String, // Base64-encoded encrypted data
    pub timestamp: i64,
}