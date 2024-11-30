use std::fs::File;
use std::io::{self, BufRead};
use std::sync::Arc;

use crate::message;
use dotenv::dotenv;
use log::{debug, error, info, warn};
use serenity::all::{ChannelId, CreateMessage, GuildId, Http, UserId};
use serenity::async_trait;
use serenity::model::channel;
use serenity::prelude::*;
use tokio::sync::{broadcast, mpsc};

pub struct DiscordBot {
    client: Arc<tokio::sync::Mutex<Client>>,
    http: Arc<Http>,
}

impl DiscordBot {
    pub async fn new(token: &str, message_tx: mpsc::Sender<message::Message>) -> Self {
        // Set gateway intents, which decides what events the bot will be notified about
        let intents = GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT;

        // Create a new instance of the Client, logging in as a bot.
        let client = Client::builder(&token, intents)
            .event_handler(Handler { message_tx })
            .await
            .expect("Failed to create client");

        // Clone the HTTP to decouple it from the client.
        // (see comment in the start() function)
        let http = client.http.clone();

        Self {
            client: Arc::new(Mutex::new(client)),
            http,
        }
    }

    /// Starts up the bot
    pub async fn start(&self) {
        // BEWARE, THE LOCK IS DROPPED AT THE END OF THE BOT'S LIFETIME.
        // TRYING TO USE .lock() ON THE CLIENT WHILE ITS RUNNING WILL
        // PEND INFINITELY.
        if let Err(err) = self.client.lock().await.start().await {
            error!("Failed to start Discord bot: {err}");
        }

        info!("Discord bot started");
    }

    /// Infinite loop that listens on the receiver and sends the message to Discord channel
    /// as soon as a message is received.
    pub async fn handle_write_discord(
        &self,
        rx: mpsc::Receiver<message::Message>,
        stop_tx: broadcast::Sender<()>,
        channel_ids: &[u64],
    ) {
        let mut stop_rx = stop_tx.subscribe();

        tokio::select! {
            _ = self.handle_write_discord_offload(rx, stop_tx, channel_ids) => {
                return;
            }
            _ = stop_rx.recv() => {
                debug!("Received stop signal");
                return;
            }
        }
    }

    async fn handle_write_discord_offload(
        &self,
        mut rx: mpsc::Receiver<message::Message>,
        stop_tx: broadcast::Sender<()>,
        channel_ids: &[u64],
    ) {
        info!("Listening for messages to SEND to Discord");
        let channels = channel_ids
            .iter()
            .map(|id| ChannelId::new(*id))
            .collect::<Vec<ChannelId>>();
        info!("REAL channels: {channels:#?}");

        // Channel index counter that will rotate.
        // u128 so that we are sure it will never overflow
        let mut counter: u128 = 0;

        // Listen infinitely
        loop {
            debug!("SENT DISCORD MESSAGES: {counter}");
            match rx.recv().await {
                Some(received_message) => {
                    debug!("Received a message to SEND to Discord");
                    let rotated_idx: usize = (counter % (channels.len() - 1) as u128) as usize;
                    counter += 1;

                    let channel = channels[rotated_idx];
                    let message_content = received_message.to_string_representation();
                    let message = CreateMessage::new().content(message_content);

                    if let Err(err) = channel.send_message(&self.http, message).await {
                        warn!("Failed to send message to Discord channel: {err}");
                    }
                }
                None => {
                    error!("Received None (channel closed): exiting the function");
                    stop_tx.send(()).unwrap();
                    debug!("Channel closed (None received): broadcast stop signal");
                    return;
                }
            }
        }
    }
}

struct Handler {
    message_tx: mpsc::Sender<message::Message>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: channel::Message) {
        // Exclude messages sent by us
        if msg.author.id == get_bot_id(ctx).await {
            return;
        }

        // Exclude all messages on other guilds
        if msg.guild_id.unwrap_or(GuildId::default()).to_string() != get_discord_guild_id() {
            return;
        }

        let received_message: String = msg.content;
        // Will be sent to the mpsc::Sender

        match message::Message::from_string(&received_message) {
            Ok(message) => {
                if let Err(err) = self.message_tx.send(message).await {
                    warn!("Failed to enqueue message from Discord: {err}");
                }
            }
            Err(err) => {
                warn!("Failed to decode message from string: {err}");
            }
        }
    }
}

/// Returns a vec of u64 of each line from a file.
pub fn read_channel_ids_file(filepath: &str) -> Vec<u64> {
    // Open the file
    let file = File::open(filepath).expect("Failed to open Discord channel IDs file");

    // Create a buffered reader
    let reader = io::BufReader::new(file);
    let mut channel_ids: Vec<u64> = Vec::new();

    for line in reader.lines() {
        let line = line.expect("Failed to read line");
        channel_ids.push(line.parse().expect("Failed to parse line as a channel ID"));
    }

    channel_ids
}

/// A lazy-initialized value because in the handler, we need the value of the botID to ignore our
/// messages, so we'll query it once and reuse it for the rest of the program's lifetime.
static BOT_ID: tokio::sync::OnceCell<UserId> = tokio::sync::OnceCell::const_new();

/// lazy-initialized Discord bot token
static DISCORD_BOT_TOKEN: once_cell::sync::OnceCell<String> = once_cell::sync::OnceCell::new();

/// lazy-initialized Discord Guild ID.
static DISCORD_GUILD_ID: once_cell::sync::OnceCell<String> = once_cell::sync::OnceCell::new();

/// Fetches the current running bot UserId and stores it into a static variable for later use.
async fn get_bot_id(ctx: Context) -> UserId {
    // Initialize the value if not already initialized
    *BOT_ID
        .get_or_init(|| async { ctx.http.get_current_user().await.unwrap().id })
        .await
}

/// Reads the Discord bot token from a .env file and initializes the static var above.
pub fn get_discord_bot_token() -> &'static str {
    DISCORD_BOT_TOKEN
        .get_or_init(|| {
            dotenv().ok();
            std::env::var("DISCORD_BOT_TOKEN")
                .expect("Failed to read DISCORD_BOT_TOKEN from a .env file")
        })
        .as_str()
}

/// Reads the Discord bot token from a .env file and initializes the static var above.
pub fn get_discord_guild_id() -> &'static str {
    DISCORD_GUILD_ID
        .get_or_init(|| {
            dotenv().ok();
            std::env::var("DISCORD_GUILD_ID")
                .expect("Failed to read DISCORD_BOT_TOKEN from a .env file")
        })
        .as_str()
}
