use chat_application::models::Message;
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

// Shared state for authentication
struct ClientState {
    authenticated: bool,
    user_id: Option<Uuid>,
}

pub async fn run_client(addr: &str) -> Result<()> {
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
                            ServerResponse::Success(msg) => {
                                println!("Success: {}", msg);
                                let mut state = state_clone.lock().await;
                                state.authenticated = true;
                                state.user_id = Some(Uuid::new_v4());
                            }
                            ServerResponse::Error(msg) => {
                                println!("Error: {}. Please try again.", msg);
                            }
                            ServerResponse::Message(msg) => {
                                let decoded = STANDARD
                                    .decode(&msg.content)
                                    .map_err(|e| anyhow::anyhow!("Base64 decode error: {}", e))?;
                                let decrypted = crypto_clone.decrypt(&key, &decoded)?;
                                println!(
                                    "Received message: {}",
                                    String::from_utf8_lossy(&decrypted)
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
            _ => {
                if !authenticated {
                    println!("Please login or register first");
                    continue;
                }
                let plaintext = input.as_bytes();
                let encrypted = crypto.encrypt(&key, plaintext)?;
                ClientMessage::Text(Message {
                    id: Uuid::new_v4(),
                    sender_id: user_id.unwrap(),
                    group_id: None,
                    content: STANDARD.encode(encrypted),
                    timestamp: chrono::Utc::now().timestamp(),
                })
            }
        };

        let msg_json = serde_json::to_string(&msg)?;
        println!("Sending: {}", msg_json);
        writer.write_all(msg_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        // Brief delay to allow server response
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