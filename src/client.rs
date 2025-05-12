use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt, AsyncBufReadExt, BufReader};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::models::Message;
use crate::crypto::Crypto;
use anyhow::Result;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use std::env;

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

pub async fn run_client(addr: &  str) -> Result<()> {
    let stream = TcpStream::connect(addr).await?;
    println!("Connected to {}", addr);

    let (mut reader, mut writer) = tokio::io::split(stream);
    let crypto = Crypto::new();
    let key = [0u8; 32]; // Dummy key for testing
    let mut user_id: Option<Uuid> = None;
    let mut authenticated = false;

    // Spawn a task to read server responses
    tokio::spawn(async move {
        let mut buf = [0; 1024];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => {
                    println!("Server disconnected");
                    break;
                }
                Ok(n) => {
                    let response = String::from_utf8_lossy(&buf[..n]);
                    if let Ok(server_response) = serde_json::from_str::<ServerResponse>(&response) {
                        match server_response {
                            ServerResponse::Prompt(msg) => println!("{}", msg),
                            ServerResponse::Success(msg) => println!("Success: {}", msg),
                            ServerResponse::Error(msg) => println!("Error: {}", msg),
                            ServerResponse::Message(msg) => {
                                let decoded = STANDARD.decode(&msg.content).map_err(|e| anyhow::anyhow!("Base64 decode error: {}", e))?;
                                let decrypted = crypto.decrypt(&key, &decoded)?;
                                println!("Received message: {}", String::from_utf8_lossy(&decrypted));
                            }
                        }
                    } else {
                        println!("Invalid response: {}", response);
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

        writer.write_all(serde_json::to_string(&msg)?.as_bytes()).await?;
        writer.write_all(b"\n").await?;

        // Update authentication status based on response
        if matches!(msg, ClientMessage::Register { .. } | ClientMessage::Login { .. }) {
            let mut buf = [0; 1024];
            let n = reader.read(&mut buf).await?;
            let response = String::from_utf8_lossy(&buf[..n]);
            if let Ok(server_response) = serde_json::from_str::<ServerResponse>(&response) {
                if let ServerResponse::Success(_) = server_response {
                    authenticated = true;
                    user_id = Some(Uuid::new_v4()); // Simplified; server should return user_id
                }
            }
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