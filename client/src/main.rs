use log::info;
use shared;
use std::error::Error;
use std::io;
use tokio::net::TcpListener;
use tokio::task;

const ADDRESS: &str = "127.0.0.1";
const PORT: u16 = 25565;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    shared::logging::init_logger();
    info!("CLIENT SIDE RUNNING...");

    let listener = TcpListener::bind(format!("{ADDRESS}:{PORT}")).await?;

    // We'll support only ONE connection at a time, so let's not spawn new async tasks each time
    // a client tries to connect.

    loop {
        let (socket, addr) = listener.accept().await?;

        // Split to socket in two OWNED parts so that we can use the socket through two functions.
        let (read_half, write_half) = socket.into_split();

        let (tx, rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);

        tokio::spawn(async move {
            shared::sockets::handle_receive_socket(read_half, tx);
        });
    }

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
