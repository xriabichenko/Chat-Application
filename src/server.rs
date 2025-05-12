use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::storage::Storage;

pub struct Server {
    storage: Arc<Mutex<Storage>>,
}

impl Server {
    pub async fn new(db_path: &str) -> anyhow::Result<Self> {
        let storage = Storage::new(db_path)?;
        Ok(Server {
            storage: Arc::new(Mutex::new(storage)),
        })
    }

    pub async fn run(&self, addr: &str) -> anyhow::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        println!("Server running on {}", addr);

        loop {
            let (mut socket, addr) = listener.accept().await?;
            println!("New connection: {}", addr);

            let storage = Arc::clone(&self.storage);
            tokio::spawn(async move {
                let mut buf = [0; 1024];
                loop {
                    let n = socket.read(&mut buf).await.unwrap();
                    if n == 0 {
                        println!("Connection closed: {}", addr);
                        break;
                    }
                    let msg = String::from_utf8_lossy(&buf[..n]);
                    println!("Received from {}: {}", addr, msg);

                    // Store message (encrypted) in database
                    // Broadcast to group members (implement later)
                    socket.write_all(msg.as_bytes()).await.unwrap();
                }
            });
        }
    }
}