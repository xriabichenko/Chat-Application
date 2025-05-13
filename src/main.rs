use dotenv::dotenv;
use std::env;
use crate::server::Server;

mod models;
mod crypto;
mod storage;
mod server;
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    env_logger::init();



    let db_path = env::var("DATABASE_URL").unwrap_or("./chat.db".to_string());
    let server_addr = env::var("SERVER_PORT").unwrap_or("127.0.0.1:8080".to_string());

    // Start server in a separate task
    let server = Server::new(&db_path).await?;
    let server_addr_clone = server_addr.clone();
    tokio::spawn(async move {
        if let Err(e) = server.run(&server_addr_clone).await {
            eprintln!("Server error: {}", e);
        }
    });


    // println!("Server running on {}", server_addr);
    tokio::signal::ctrl_c().await?;
    println!("Shutting down server...");

    Ok(())
}