use tokio::net::TcpListener;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use crate::storage::Storage;
use crate::models::{Message, User, Group};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::collections::HashMap;
use anyhow::Result;

#[derive(Serialize, Deserialize)]
enum ClientMessage {
    Register { username: String, public_key: String },
    Login { username: String, public_key: String },
    Text(Message),
    Send { receiver_username: String, message: Message },
    CreateGroup { group_name: String, member_usernames: Vec<String> }, // Создание группы
    SendToGroup { group_id: Uuid, message: Message },
}

#[derive(Serialize, Deserialize)]
enum ServerResponse {
    Prompt(String),
    Success { message: String, user_id: Uuid },
    Error(String),
    Message(Message),
}

pub struct Server {
    storage: Arc<Mutex<Storage>>,
    clients: Arc<Mutex<HashMap<Uuid, mpsc::Sender<ServerResponse>>>>,
}

impl Server {
    pub async fn new(db_path: &str) -> Result<Self> {
        let storage = Storage::new(db_path)?;
        Ok(Server {
            storage: Arc::new(Mutex::new(storage)),
            clients: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn run(&self, addr: &str) -> Result<()> {
        let listener = TcpListener::bind(addr).await?;
        println!("Server running on {}", addr);

        loop {
            let (socket, addr) = listener.accept().await?;
            println!("New connection: {}", addr);

            let storage = Arc::clone(&self.storage);
            let clients = Arc::clone(&self.clients);

            tokio::spawn(async move {
                if let Err(e) = handle_client(socket, storage, clients).await {
                    println!("Error handling client: {}", e);
                }
            });
        }
    }
}

async fn handle_client(
    socket: tokio::net::TcpStream,
    storage: Arc<Mutex<Storage>>,
    clients: Arc<Mutex<HashMap<Uuid, mpsc::Sender<ServerResponse>>>>,
) -> Result<()> {
    let (reader, mut socket_write) = tokio::io::split(socket);
    let mut reader = BufReader::new(reader);
    let (tx, mut rx) = mpsc::channel::<ServerResponse>(100);

    tokio::spawn(async move {
        while let Some(response) = rx.recv().await {
            let response_json = serde_json::to_string(&response)?;
            println!("Sending to client: {}", response_json); // Debug: log sent response
            socket_write.write_all(response_json.as_bytes()).await?;
            socket_write.write_all(b"\n").await?;
            socket_write.flush().await?;
        }
        Ok::<(), anyhow::Error>(())
    });

    tx.send(ServerResponse::Prompt(
        "Please send 'login <username> <public_key>', 'register <username> <public_key>', 'send <username> <message>', 'create_group <group_name> <username1> <username2> ...', or 'send_group <group_id> <message>'".to_string(),
    ))
        .await?;

    let mut user_id: Option<Uuid> = None;
    let mut authenticated = false;
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                println!("Connection closed for user {:?}", user_id);
                if let Some(id) = user_id {
                    clients.lock().await.remove(&id);
                }
                break;
            }
            Ok(_) => {
                println!("Received from user {:?}: {}", user_id, line);
                if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&line) {
                    match client_msg {
                        ClientMessage::Register { username, public_key } => {
                            let mut storage = storage.lock().await;
                            let user = User {
                                id: Uuid::new_v4(),
                                username: username.clone(),
                                public_key,
                            };
                            match storage.save_user(&user) {
                                Ok(_) => {
                                    user_id = Some(user.id);
                                    authenticated = true;
                                    let mut clients = clients.lock().await;
                                    clients.insert(user.id, tx.clone());
                                    tx.send(ServerResponse::Success {
                                        message: format!("Registered and logged in as {}", username),
                                        user_id: user.id,
                                    })
                                        .await?;
                                }
                                Err(e) => {
                                    tx.send(ServerResponse::Error(format!(
                                        "Registration failed: {}",
                                        e
                                    )))
                                        .await?;
                                }
                            }
                        }
                        ClientMessage::Login { username, public_key } => {
                            let storage = storage.lock().await;
                            match storage.get_user_by_username(&username) {
                                Ok(user) if user.public_key == public_key => {
                                    user_id = Some(user.id);
                                    authenticated = true;
                                    let mut clients = clients.lock().await;
                                    clients.insert(user.id, tx.clone());
                                    let response = ServerResponse::Success {
                                        message: format!("Logged in as {}", username),
                                        user_id: user.id,
                                    };
                                    tx.send(response).await?;
                                }
                                Ok(_) => {
                                    tx.send(ServerResponse::Error(
                                        "Invalid public key".to_string(),
                                    ))
                                        .await?;
                                }
                                Err(_) => {
                                    tx.send(ServerResponse::Error("User not found".to_string()))
                                        .await?;
                                }
                            }
                        }
                        ClientMessage::Text(msg) => {
                            if !authenticated {
                                tx.send(ServerResponse::Error(
                                    "Not authenticated".to_string(),
                                ))
                                    .await?;
                                continue;
                            }
                            if user_id != Some(msg.sender_id) {
                                tx.send(ServerResponse::Error(
                                    "Invalid sender_id".to_string(),
                                ))
                                    .await?;
                                continue;
                            }
                            let storage = storage.lock().await;
                            storage.save_message(&msg)?;
                            tx.send(ServerResponse::Message(msg.clone())).await?;
                        }
                        ClientMessage::Send { receiver_username, mut message } => {
                            if !authenticated {
                                tx.send(ServerResponse::Error(
                                    "Not authenticated".to_string(),
                                ))
                                    .await?;
                                continue;
                            }
                            if user_id != Some(message.sender_id) {
                                tx.send(ServerResponse::Error(
                                    "Invalid sender_id".to_string(),
                                ))
                                    .await?;
                                continue;
                            }
                            let storage = storage.lock().await;
                            match storage.get_user_by_username(&receiver_username) {
                                Ok(receiver) => {
                                    message.receiver_id = Some(receiver.id);
                                    storage.save_message(&message)?;
                                    let clients = clients.lock().await;
                                    if let Some(receiver_tx) = clients.get(&receiver.id) {
                                        receiver_tx
                                            .send(ServerResponse::Message(message.clone()))
                                            .await?;
                                        tx.send(ServerResponse::Success {
                                            message: format!("Message sent to {}", receiver_username),
                                            user_id: receiver.id,
                                        })
                                            .await?;
                                    } else {
                                        tx.send(ServerResponse::Error(
                                            "Recipient is offline".to_string(),
                                        ))
                                            .await?;
                                    }
                                }
                                Err(_) => {
                                    tx.send(ServerResponse::Error(
                                        "Recipient not found".to_string(),
                                    ))
                                        .await?;
                                }
                            }
                        }
                        ClientMessage::CreateGroup { group_name, member_usernames } => {
                            if !authenticated {
                                tx.send(ServerResponse::Error("Not authenticated".to_string())).await?;
                                continue;
                            }
                            let mut storage = storage.lock().await;
                            let mut members = vec![user_id.unwrap()];

                            // Найти ID пользователей по именам
                            for username in member_usernames {
                                match storage.get_user_by_username(&username) {
                                    Ok(user) => members.push(user.id),
                                    Err(_) => {
                                        tx.send(ServerResponse::Error(format!("User {} not found", username))).await?;
                                        continue;
                                    }
                                }
                            }
                            // Создать группу
                            let group = Group {
                                id: Uuid::new_v4(),
                                name: group_name,
                                members,
                            };
                            storage.save_group(&group)?;

                            tx.send(ServerResponse::Success {
                                message: format!("Group {} created with {} members (group_id: {})", group.name, group.members.len(), group.id),
                                user_id: user_id.unwrap(),
                            }).await?;
                        }
                        ClientMessage::SendToGroup { group_id, mut message } => {
                            if !authenticated {
                                tx.send(ServerResponse::Error("Not authenticated".to_string())).await?;
                                continue;
                            }
                            if user_id != Some(message.sender_id) {
                                tx.send(ServerResponse::Error("Invalid sender_id".to_string())).await?;
                                continue;
                            }

                            let storage = storage.lock().await;
                            match storage.get_group_by_id(&group_id) {
                                Ok(group) => {
                                    if !group.members.contains(&message.sender_id) {
                                        tx.send(ServerResponse::Error("You are not a member of this group".to_string())).await?;
                                        continue;
                                    }
                                    message.group_id = Some(group_id);
                                    storage.save_message(&message)?;

                                    let clients = clients.lock().await;
                                    for member_id in group.members {
                                        if let Some(member_tx) = clients.get(&member_id) {
                                            member_tx.send(ServerResponse::Message(message.clone())).await?;
                                        }
                                    }
                                    tx.send(ServerResponse::Success {
                                        message: format!("Message sent to group {}", group_id),
                                        user_id: user_id.unwrap(),
                                    }).await?;
                                }
                                Err(_) => {
                                    tx.send(ServerResponse::Error("Group not found".to_string())).await?;
                                }
                            }
                        }
                    }
                } else {
                    tx.send(ServerResponse::Error(
                        "Invalid message format".to_string(),
                    ))
                        .await?;
                }
            }
            Err(e) => {
                println!("Error reading from socket: {}", e);
                if let Some(id) = user_id {
                    clients.lock().await.remove(&id);
                }
                return Err(e.into());
            }
        }
    }
    Ok(())
}