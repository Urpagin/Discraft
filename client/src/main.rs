use log::info;
use std::error::Error;
use tokio::net::TcpListener;

const ADDRESS: &str = "127.0.0.1";
const PORT: u16 = 25565;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    shared::logging::init_logger();
    info!("CLIENT SIDE RUNNING...");

    let listener = TcpListener::bind(format!("{ADDRESS}:{PORT}")).await?;

    // We'll support only ONE connection at a time, so let's not spawn new async tasks each time
    // a client tries to connect.

    let mut conn_counter: u64 = 0;

    loop {
        let (socket, addr) = listener.accept().await?;
        conn_counter += 1;
        info!("Connected to client #{conn_counter}: {socket:?} - {addr}");

        // Split to socket in two OWNED parts so that we can use the socket through two functions.
        let (read_half, write_half) = socket.into_split();

        // ----------------------------- MC Client -> Discord -----------------------------

        let (tcp_tx, tcp_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);

        // Handle MC Client packets
        tokio::spawn(async move {
            shared::sockets::handle_receive_socket(read_half, tcp_tx).await;
        });

        // Send MC Client packets to Discord
        tokio::spawn(async move {
            // TODO: todo (takes tcp_rx)
        });

        // ----------------------------- Discord -> MC Client -----------------------------

        let (discord_tx, discord_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);

        // Handle new Discord messages
        tokio::spawn(async move {
            // TODO: todo (takes discord_tx)
        });

        // Send Discord messages to MC Client
        tokio::spawn(async move {
            shared::sockets::handle_channel_to_socket(write_half, tcp_rx).await;
        });

        // --------------------------------------------------------------------------------
    }
}
