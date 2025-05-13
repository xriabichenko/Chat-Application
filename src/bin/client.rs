use chat_application::models::{Message, File};
use chat_application::crypto::Crypto;
use tokio::net::TcpStream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use anyhow::Result;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use std::env;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};
use std::fs;
use chrono::{DateTime, Utc};

#[derive(Serialize, Deserialize)]
enum ClientMessage {
    Register { username: String, public_key: String },
    Login { username: String, public_key: String },
    Text(Message),
    Send { receiver_username: String, message: Message },
    CreateGroup { group_name: String, member_usernames: Vec<String> },
    SendToGroup { group_id: Uuid, message: Message },
    SendFile { receiver_username: String, file: File },
    SendFileToGroup { group_id: Uuid, file: File },
}

#[derive(Serialize, Deserialize)]
enum ServerResponse {
    Prompt(String),
    Success { message: String, user_id: Uuid },
    Error(String),
    Message(Message),
    File(File),
}

// Shared state for authentication
struct ClientState {
    authenticated: bool,
    user_id: Option<Uuid>,
}

pub async fn run_client(addr:  &str) -> Result<()> {
    let stream = TcpStream::connect(addr).await?;
    println!("Connected to {}", addr);

    let (reader, mut writer) = tokio::io::split(stream);
    let crypto = Crypto::new();
    let crypto_clone = crypto.clone();
    let key = [0u8; 32]; // Dummy key for testing

    // Shared state
    let state = Arc::new(Mutex::new(ClientState {
        authenticated: false,
        user_id: None,
    }));
    let state_clone = Arc::clone(&state);

    // Spawn a task to read server responses
    tokio::spawn(async move {
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    println!("Server disconnected");
                    break;
                }
                Ok(n) => {
                    println!("Raw server response ({} bytes): '{}'", n, line);
                    if let Ok(server_response) = serde_json::from_str::<ServerResponse>(&line) {
                        match server_response {
                            ServerResponse::Prompt(msg) => println!("Prompt: {}", msg),
                            ServerResponse::Success { message, user_id } => {
                                println!("Success: {} (user_id: {})", message, user_id);
                                let mut state = state_clone.lock().await;
                                state.authenticated = true;
                                state.user_id = Some(user_id);
                            }
                            ServerResponse::Error(msg) => {
                                println!("Error: {}. Please try again.", msg);
                            }
                            ServerResponse::Message(msg) => {
                                let decoded = STANDARD
                                    .decode(&msg.content)
                                    .map_err(|e| anyhow::anyhow!("Base64 decode error: {}", e))?;
                                let decrypted = crypto_clone.decrypt(&key, &decoded)?;
                                let sender_info = if msg.group_id.is_some() {
                                    format!("from group {}", msg.group_id.unwrap())
                                } else {
                                    format!("from sender_id: {}", msg.sender_id)
                                };
                                let timestamp: DateTime<Utc> = DateTime::from_timestamp(msg.timestamp, 0).unwrap_or_default();
                                println!(
                                    "Received message: {} ({} at {})",
                                    String::from_utf8_lossy(&decrypted),
                                    sender_info,
                                    timestamp.to_rfc3339()
                                );
                            }
                            ServerResponse::File(file) => {
                                let decoded = STANDARD
                                    .decode(&file.data)
                                    .map_err(|e| anyhow::anyhow!("Base64 decode error: {}", e))?;
                                let decrypted = crypto_clone.decrypt(&key, &decoded)?;
                                let output_path = format!("received_{}", file.filename);
                                fs::write(&output_path, decrypted)?;
                                let sender_info = if file.group_id.is_some() {
                                    format!("from group {}", file.group_id.unwrap())
                                } else {
                                    format!(
                                        "from sender_id: {} to receiver_id: {}",
                                        file.sender_id,
                                        file.receiver_id.map_or("unknown".to_string(), |id| id.to_string())
                                    )
                                };
                                let timestamp: DateTime<Utc> = DateTime::from_timestamp(file.timestamp, 0).unwrap_or_default();
                                println!(
                                    "Received file: {} saved as {} ({} at {})",
                                    file.filename, output_path, sender_info, timestamp.to_rfc3339()
                                );
                            }
                        }
                    } else {
                        println!("Invalid response: {}", line);
                    }
                }
                Err(e) => {
                    println!("Error reading: {}", e);
                    break;
                }
            }
        }
        Ok::<(), anyhow::Error>(())
    });

    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut input = String::new();

    loop {
        print!("> ");
        std::io::Write::flush(&mut std::io::stdout())?;

        input.clear();
        reader.read_line(&mut input).await?;
        let input = input.trim();

        if input.eq_ignore_ascii_case("exit") {
            println!("Exiting client...");
            break;
        }

        if input.is_empty() {
            continue;
        }

        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let state = state.lock().await;
        let authenticated = state.authenticated;
        let user_id = state.user_id;
        drop(state);

        let msg = match parts[0].to_lowercase().as_str() {
            "register" if parts.len() >= 3 => {
                let username = parts[1].to_string();
                let public_key = parts[2..].join(" ");
                ClientMessage::Register { username, public_key }
            }
            "login" if parts.len() >= 3 => {
                let username = parts[1].to_string();
                let public_key = parts[2..].join(" ");
                ClientMessage::Login { username, public_key }
            }
            "send" if parts.len() >= 3 => {
                if !authenticated {
                    println!("Please login or register first");
                    continue;
                }
                let receiver_username = parts[1].to_string();
                let message_content = parts[2..].join(" ");
                let plaintext = message_content.as_bytes();
                let encrypted = crypto.encrypt(&key, plaintext)?;
                ClientMessage::Send {
                    receiver_username,
                    message: Message {
                        id: Uuid::new_v4(),
                        sender_id: user_id.unwrap(),
                        receiver_id: None,
                        group_id: None,
                        content: STANDARD.encode(encrypted),
                        timestamp: chrono::Utc::now().timestamp(),
                    },
                }
            }
            "create_group" if parts.len() >= 3 => {
                if !authenticated {
                    println!("Please login or register first");
                    continue;
                }
                let group_name = parts[1].to_string();
                let member_usernames = parts[2..].iter().map(|s| s.to_string()).collect();
                ClientMessage::CreateGroup { group_name, member_usernames }
            }
            "send_group" if parts.len() >= 3 => {
                if !authenticated {
                    println!("Please login or register first");
                    continue;
                }
                let group_id = Uuid::parse_str(parts[1]).map_err(|e| {
                    println!("Invalid group ID: {}", e);
                    anyhow::anyhow!("Invalid group ID")
                })?;
                let message_content = parts[2..].join(" ");
                let plaintext = message_content.as_bytes();
                let encrypted = crypto.encrypt(&key, plaintext)?;
                ClientMessage::SendToGroup {
                    group_id,
                    message: Message {
                        id: Uuid::new_v4(),
                        sender_id: user_id.unwrap(),
                        receiver_id: None,
                        group_id: Some(group_id),
                        content: STANDARD.encode(encrypted),
                        timestamp: chrono::Utc::now().timestamp(),
                    },
                }
            }
            "send_file" if parts.len() == 3 => {
                if !authenticated {
                    println!("Please login or register first");
                    continue;
                }
                let receiver_username = parts[1].to_string();
                let file_path = parts[2].to_string();
                println!("Attempting to read file: {}", file_path);
                // println!("username: {},  file_path{}", receiver_username, file_path);
                let file_data = fs::read(&file_path).map_err(|e| {
                    println!("Failed to read file: {}", e);
                    anyhow::anyhow!("Failed to read file")
                })?;
                let encrypted = crypto.encrypt(&key, &file_data)?;
                let filename = file_path
                    .split('/')
                    .last()
                    .or_else(|| file_path.split('\\').last())
                    .unwrap_or("unknown")
                    .to_string();
                ClientMessage::SendFile {
                    receiver_username,
                    file: File {
                        id: Uuid::new_v4(),
                        sender_id: user_id.unwrap(),
                        receiver_id: None, // Server will set
                        group_id: None,
                        filename,
                        data: STANDARD.encode(encrypted),
                        timestamp: chrono::Utc::now().timestamp(),
                    },
                }
            }
            "send_file_group" if parts.len() == 3 => {
                if !authenticated {
                    println!("Please login or register first");
                    continue;
                }
                let group_id = Uuid::parse_str(parts[1]).map_err(|e| {
                    println!("Invalid group ID: {}", e);
                    anyhow::anyhow!("Invalid group ID")
                })?;
                let file_path = parts[2].to_string();
                let file_data = fs::read(&file_path).map_err(|e| {
                    println!("Failed to read file: {}", e);
                    anyhow::anyhow!("Failed to read file")
                })?;
                let encrypted = crypto.encrypt(&key, &file_data)?;
                let filename = file_path
                    .split('/')
                    .last()
                    .or_else(|| file_path.split('\\').last())
                    .unwrap_or("unknown")
                    .to_string();
                ClientMessage::SendFileToGroup {
                    group_id,
                    file: File {
                        id: Uuid::new_v4(),
                        sender_id: user_id.unwrap(),
                        receiver_id: None,
                        group_id: Some(group_id),
                        filename,
                        data: STANDARD.encode(encrypted),
                        timestamp: chrono::Utc::now().timestamp(),
                    },
                }
            }
            _ => {
                println!("Unknown command. Use 'register <username> <public_key>', 'login <username> <public_key>', 'send <username> <message>', 'create_group <group_name> <username1> <username2> ...', 'send_group <group_id> <message>', 'send_file <username> <filepath>', or 'send_file_group <group_id> <filepath>'");
                continue;
            }
        };

        let msg_json = serde_json::to_string(&msg)?;
        // println!("Sending: {}", msg_json);
        writer.write_all(msg_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        if matches!(msg, ClientMessage::Register { .. } | ClientMessage::Login { .. }) {
            sleep(Duration::from_millis(100)).await;
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| env::var("SERVER_PORT").unwrap_or("127.0.0.1:8080".to_string()));

    run_client(&addr).await
}