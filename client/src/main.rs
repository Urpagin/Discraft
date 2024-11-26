use std::error::Error;
use std::io;
use tokio::net::TcpListener;
use tokio::task;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Listen for incomming TCP connections
    let tcp_task = task::spawn(async move {
        let _ = listen_tcp().await;
    });

    // Listen for Discord messages
    let discord_task = task::spawn(async move {
        let _ = listen_discord().await;
    });

    // Wait for both tasks to complete (which should not happen if they're infinite loops)
    let _ = tokio::try_join!(tcp_task, discord_task);

    Ok(())
}

async fn listen_tcp() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:25565").await?;

    loop {
        let (socket, var) = listener.accept().await?;
        println!("Got socket: {socket:#?} and var: {var:#?}");
    }
}

async fn listen_discord() {
    println!("Hello from listen_discord()");
}
