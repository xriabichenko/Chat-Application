use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt, AsyncBufReadExt, BufReader};
use anyhow::Result;
use std::env;

async fn run_client(addr: &str) -> Result<()> {
    let mut stream = TcpStream::connect(addr).await?;
    println!("Connected to {}", addr);

    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut input = String::new();

    loop {
        print!("Enter message (type 'exit' to quit): ");
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

        let msg = format!("{}\n", input);
        stream.write_all(msg.as_bytes()).await?;
        println!("Sent: {}", input);

        let mut buf = [0; 1024];
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            println!("Server closed the connection");
            break;
        }
        let response = String::from_utf8_lossy(&buf[..n]);
        println!("Received: {}", response);
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