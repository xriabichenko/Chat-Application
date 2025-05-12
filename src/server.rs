use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use crate::storage::Storage;
use crate::models::{Message, User};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::collections::HashMap;
use anyhow::Result;

#[derive(Serialize, Deserialize)]
enum ClientMessage {
    Register { username: String, public_key: String },
    Login { username: String, public_key: String },
    Text(Message),
}

#[derive(Serialize, Deserialize)]
enum ServerResponse {
    Prompt(String),
    Success(String),
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
    let (mut socket_read, mut socket_write) = tokio::io::split(socket);
    let mut buf = [0; 1024];
    let (tx, mut rx) = mpsc::channel::<ServerResponse>(100);

    tokio::spawn(async move {
        while let Some(response) = rx.recv().await {
            if socket_write
                .write_all(serde_json::to_string(&response)?.as_bytes())
                .await
                .is_err()
            {
                break;
            }
        }
        Ok::<(), anyhow::Error>(())
    });

    // Send login/register prompt
    tx.send(ServerResponse::Prompt(
        "Please send 'login <username> <public_key>' or 'register <username> <public_key>'".to_string(),
    ))
        .await?;

    let mut user_id: Option<Uuid> = None;
    let mut authenticated = false;

    loop {
        match socket_read.read(&mut buf).await {
            Ok(0) => {
                println!("Connection closed");
                if let Some(id) = user_id {
                    clients.lock().await.remove(&id);
                }
                break;
            }
            Ok(n) => {
                let data = String::from_utf8_lossy(&buf[..n]);
                if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&data) {
                    let mut storage = storage.lock().await;
                    match client_msg {
                        ClientMessage::Register { username, public_key } => {
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
                                    tx.send(ServerResponse::Success(format!("Registered and logged in as {}", username)))
                                        .await?;
                                }
                                Err(e) => {
                                    tx.send(ServerResponse::Error(format!("Registration failed: {}", e)))
                                        .await?;
                                }
                            }
                        }
                        ClientMessage::Login { username, public_key } => {
                            match storage.get_user_by_username(&username) {
                                Ok(user) if user.public_key == public_key => {
                                    user_id = Some(user.id);
                                    authenticated = true;
                                    let mut clients = clients.lock().await;
                                    clients.insert(user.id, tx.clone());
                                    tx.send(ServerResponse::Success(format!("Logged in as {}", username)))
                                        .await?;
                                }
                                Ok(_) => {
                                    tx.send(ServerResponse::Error("Invalid public key".to_string()))
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
                                tx.send(ServerResponse::Error("Not authenticated".to_string()))
                                    .await?;
                                continue;
                            }
                            storage.save_message(&msg)?;
                            tx.send(ServerResponse::Message(msg.clone())).await?;
                        }
                    }
                } else {
                    tx.send(ServerResponse::Error("Invalid message format".to_string()))
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