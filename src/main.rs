mod cli;
mod discord;
mod logging;
mod message;
mod sockets;

use log::debug;
use log::error;
use log::info;
use log::warn;
use std::error::Error;
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Receiver;
use tokio::sync::Mutex;

/// Which side we are running on
///
/// Client: MC Client <-> us <-> Discord
/// Server: Discord <-> us <-> MC Server
static CURRENT_SIDE: OnceLock<cli::Mode> = OnceLock::new();

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Init logging
    logging::init_logger();

    // Init the current side (client or server)
    init_side();

    // Channel that is meant to signal to stop listening (TCP and Discord)
    // when there is a disconnection for example.
    // It should stop all awaiting async tasks.
    let (stop_tx, _) = broadcast::channel::<()>(16);

    // Start the Discord bot
    let (discord_tx, discord_rx) = mpsc::channel::<message::Message>(64);
    let discord_rx = Arc::new(Mutex::new(discord_rx)); // Wrap receiver in Arc<Mutex>

    let bot: Arc<discord::DiscordBot> = init_discord_bot(discord_tx, stop_tx.clone()).await;

    match CURRENT_SIDE.get().unwrap() {
        cli::Mode::Server { .. } => server(stop_tx, bot, discord_rx).await,
        cli::Mode::Client { .. } => client(stop_tx, bot, discord_rx).await,
    }
}

async fn init_discord_bot(
    sender: mpsc::Sender<message::Message>,
    stop_tx: broadcast::Sender<()>,
) -> Arc<discord::DiscordBot> {
    let current_side = CURRENT_SIDE.get().unwrap().clone();
    let bot = Arc::new(discord::DiscordBot::new(current_side, sender).await);

    let bot_clone = Arc::clone(&bot);
    tokio::spawn(async move {
        debug!("Inside the bot.start() async task");
        bot_clone.start().await;

        error!("Bot exited. Broadcasting stop signal");
        stop_tx.send(()).unwrap();
    });

    info!("Discord bot started");

    bot
}

/// Initializes the current side on which the program will run
fn init_side() {
    CURRENT_SIDE.get_or_init(|| cli::parse().mode);

    match CURRENT_SIDE.get().unwrap() {
        cli::Mode::Server { .. } => info!("[ SERVER SIDE RUNNING ]\n"),
        cli::Mode::Client { .. } => info!("[ CLIENT SIDE RUNNING ]\n"),
    }
}

/// Client-side logic
async fn client(
    stop_tx: broadcast::Sender<()>,
    bot: Arc<discord::DiscordBot>,
    discord_rx: Arc<Mutex<Receiver<message::Message>>>,
) -> Result<(), Box<dyn Error>> {
    const LISTENING_ADDR: &str = "0.0.0.0";
    const LISTENING_PORT: u16 = 25565;

    let listener = TcpListener::bind(format!("{LISTENING_ADDR}:{LISTENING_PORT}")).await?;

    let mut conn_counter: u64 = 0;

    loop {
        info!("Listening on {LISTENING_ADDR}:{LISTENING_PORT}...");

        let (socket, addr) = listener.accept().await?;
        info!("Connected to client #{conn_counter}: {addr}");
        conn_counter += 1;

        // Split to socket in two OWNED parts so that we can use the socket through two functions.
        let (read_half, write_half) = socket.into_split();

        // MC Client -> Discord channels
        let (tcp_tx, tcp_rx) = mpsc::channel::<message::Message>(64);

        // Receives TCP packets from the MC Client.
        let tcp_tx_clone = tcp_tx.clone();
        let stop_tx_clone = stop_tx.clone();
        let handle_receive_tcp = tokio::spawn(async move {
            debug!("Inside the handle_receive_socket async task");

            sockets::handle_receive_socket(
                read_half,
                tcp_tx_clone,
                stop_tx_clone,
                message::MessageDirection::Serverbound,
            )
            .await;
        });

        // Send MC Client packets to Discord
        let channel_ids: Vec<u64> = discord::read_channel_ids_file("channel_ids.txt");
        debug!("Discord channel IDs: {channel_ids:#?}");

        let bot_clone = Arc::clone(&bot);
        let stop_tx_clone2 = stop_tx.clone();
        let handle_write_discord = tokio::spawn(async move {
            debug!("Inside the handle_write_discord async task");
            bot_clone
                .handle_write_discord(tcp_rx, stop_tx_clone2, &channel_ids)
                .await;
        });

        // Sends received Discord messages to the MC Server through TCP.
        let stop_tx_clone3 = stop_tx.clone();
        let discord_rx_clone = Arc::clone(&discord_rx);
        let handle_write_tcp = tokio::spawn(async move {
            sockets::handle_channel_to_socket(write_half, discord_rx_clone, stop_tx_clone3).await;
        });

        if let Err(err) =
            tokio::try_join!(handle_receive_tcp, handle_write_discord, handle_write_tcp)
        {
            error!("Error in one of the connection tasks: {:?}", err);
        }

        info!("--- CONNECTION CLOSED ---");
    }
}

const SERVER_ADDRESS: &str = "82.66.201.61";
const SERVER_PORT: u16 = 25565;

/// Server-side logic
async fn server(
    stop_tx: broadcast::Sender<()>,
    bot: Arc<discord::DiscordBot>,
    discord_rx: Arc<Mutex<Receiver<message::Message>>>,
) -> Result<(), Box<dyn Error>> {
    let mut conn_counter: u64 = 0;

    loop {
        // Listen for a message that's serverbound (us)
        let discord_msg: message::Message = {
            let mut rx_guard = discord_rx.lock().await;
            match rx_guard.recv().await {
                Some(msg) => msg,
                None => {
                    warn!("Error receiving discord message from closed mpsc channel, got None");
                    continue;
                }
            }
        };

        // Connect to the server
        let mut socket = TcpStream::connect(format!("{SERVER_ADDRESS}:{SERVER_PORT}")).await?;

        // Send the first message
        if let Err(err) = socket.write_all(discord_msg.to_bytes()).await {
            error!("Failed to send first packet to MC Server: {err}");
            continue;
        }

        // Split to socket in two OWNED parts so that we can use the socket through two functions.
        let (read_half, write_half) = socket.into_split();
        info!("Connection #{conn_counter} established with {SERVER_ADDRESS}:{SERVER_PORT}");
        conn_counter += 1;

        // Sends received Discord messages to the MC Server through TCP.
        let stop_tx_clone3 = stop_tx.clone();
        let discord_rx_clone = Arc::clone(&discord_rx);
        let handle_write_tcp = tokio::spawn(async move {
            sockets::handle_channel_to_socket(write_half, discord_rx_clone, stop_tx_clone3).await;
        });

        // MC Client -> Discord channels
        let (tcp_tx, tcp_rx) = mpsc::channel::<message::Message>(64);

        // Receives TCP packets from the MC Server.
        let tcp_tx_clone = tcp_tx.clone();
        let stop_tx_clone = stop_tx.clone();
        let handle_receive_tcp = tokio::spawn(async move {
            debug!("Inside the handle_receive_socket async task");

            let messages_direction = match CURRENT_SIDE.get().unwrap() {
                cli::Mode::Server { .. } => message::MessageDirection::Serverbound,
                cli::Mode::Client { .. } => message::MessageDirection::Clientbound,
            };

            sockets::handle_receive_socket(
                read_half,
                tcp_tx_clone,
                stop_tx_clone,
                messages_direction,
            )
            .await;
        });

        // Send MC Client packets to Discord
        let channel_ids: Vec<u64> = discord::read_channel_ids_file("channel_ids.txt");
        debug!("Discord channel IDs: {channel_ids:#?}");

        let bot_clone = Arc::clone(&bot);
        let stop_tx_clone2 = stop_tx.clone();
        let handle_write_discord = tokio::spawn(async move {
            debug!("Inside the handle_write_discord async task");
            bot_clone
                .handle_write_discord(tcp_rx, stop_tx_clone2, &channel_ids)
                .await;
        });

        if let Err(err) =
            tokio::try_join!(handle_receive_tcp, handle_write_discord, handle_write_tcp)
        {
            error!("Error in one of the connection tasks: {:?}", err);
        }

        info!("--- CONNECTION CLOSED ---");
    }
}
