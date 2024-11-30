use log::debug;
use log::error;
use log::info;
use shared::discord;
use shared::message;
use shared::sockets;
use std::error::Error;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

const ADDRESS: &str = "127.0.0.1";
const PORT: u16 = 25565;

async fn get_bot(
    sender: mpsc::Sender<message::Message>,
    stop_tx: broadcast::Sender<()>,
) -> Arc<discord::DiscordBot> {
    let token = discord::get_discord_bot_token();
    let bot = Arc::new(shared::discord::DiscordBot::new(&token, sender).await);

    let bot_clone = Arc::clone(&bot);
    tokio::spawn(async move {
        debug!("Inside the bot.start() async task");
        bot_clone.start().await;

        error!("Bot exited. Broadcasting stop signal");
        stop_tx.send(()).unwrap();
    });

    bot
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Init logging
    shared::logging::init_logger();
    info!("CLIENT SIDE RUNNING...");

    // Will stop all async tasks when the connection is closed
    let (stop_tx, _) = broadcast::channel::<()>(16);

    // Start the Discord bot
    let (discord_tx, discord_rx) = mpsc::channel::<message::Message>(64);
    let discord_rx = Arc::new(Mutex::new(discord_rx)); // Wrap receiver in Arc<Mutex>

    let bot = get_bot(discord_tx, stop_tx.clone()).await;
    info!("Discord bot started");

    let mut conn_counter: u64 = 0;

    let listener = TcpListener::bind(format!("{ADDRESS}:{PORT}")).await?;

    loop {
        info!("Listening for a TCP connection...");
        let (socket, addr) = listener.accept().await?;
        conn_counter += 1;
        info!("Connected to client #{conn_counter}: {addr}");

        // Split to socket in two OWNED parts so that we can use the socket through two functions.
        let (read_half, write_half) = socket.into_split();
        let (tcp_tx, tcp_rx) = mpsc::channel::<message::Message>(64);

        // Receives TCP packets from the MC Client.
        let tcp_tx_clone = tcp_tx.clone();
        let stop_tx_clone = stop_tx.clone();
        let handle_receive_tcp = tokio::spawn(async move {
            debug!("Inside the handle_receive_socket async task");
            shared::sockets::handle_receive_socket(
                read_half,
                tcp_tx_clone,
                stop_tx_clone,
                message::MessageDirection::Clientbound,
            )
            .await;
        });

        let channel_ids: Vec<u64> = discord::read_channel_ids_file("channel_ids.txt");
        debug!("Discord channel IDs: {channel_ids:#?}");
        // Send MC Client packets to Discord
        let bot_clone = Arc::clone(&bot);

        let stop_tx_clone2 = stop_tx.clone();
        let handle_write_discord = tokio::spawn(async move {
            debug!("Inside the handle_write_discord async task");
            bot_clone
                .handle_write_discord(tcp_rx, stop_tx_clone2, &channel_ids)
                .await;
        });

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

        info!("END OF LOOP");
    }
}
