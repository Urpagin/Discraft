use std::fs::File;
use std::io::{self, BufRead};
use std::sync::Arc;
use std::time::Instant;

use crate::message::MessageDirection;
use crate::{message, Side};
use dotenv::dotenv;
use log::{debug, error, info, warn};
use serenity::all::{ChannelId, CreateMessage, Http, UserId};
use serenity::async_trait;
use serenity::model::channel;
use serenity::prelude::*;
use tokio::sync::{broadcast, mpsc};

pub struct DiscordBot {
    client: Arc<tokio::sync::Mutex<Client>>,
    http: Arc<Http>,
}

impl DiscordBot {
    /// Not 2000 because my code is flawed and does not account for any header data when
    /// partitioning.
    const MAX_MESSAGE_LENGTH_ALLOWED: usize = 1900;

    pub async fn new(
        side: crate::Side,
        token: &str,
        message_tx: mpsc::Sender<message::Message>,
    ) -> Self {
        // Launch cache cleanup async task (cleanup every X seconds)
        cache::cleanup_task().await;

        // Set gateway intents, which decides what events the bot will be notified about
        let intents = GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT;

        // Create a new instance of the Client, logging in as a bot.
        let client = Client::builder(token, intents)
            .event_handler(Handler { message_tx, side })
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
            _ = self.handle_write_discord_offload(rx, stop_tx, channel_ids) => {}
            _ = stop_rx.recv() => { debug!("Received stop signal"); return; }
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

        // Channel index counter that will rotate.
        // u128 so that we are sure it will never overflow
        let mut counter: u128 = 0;

        // Listen infinitely
        loop {
            match rx.recv().await {
                Some(received_message) => {
                    debug!("Received a message to SEND to Discord");
                    let rotated_idx: usize = (counter % channels.len() as u128) as usize;

                    match make_partitions(received_message) {
                        Ok(partitions) => {
                            for msg in partitions {
                                let channel = channels[rotated_idx];
                                if let Err(err) =
                                    channel.send_message(&self.http, msg.clone()).await
                                {
                                    warn!("Failed to send message to Discord channel: {err}");
                                    warn!("Message info: {msg:?}");
                                } else {
                                    debug!("SENT A MESSAGE TO DISCORD");
                                }
                                counter += 1;
                            }
                        }
                        Err(err) => {
                            error!("Failed to partition message: {err}. Sending stop signal...");
                            stop_tx.send(()).unwrap();
                            return;
                        }
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

/// Partitions the received message if it's too big to be sent to Discord as one.
fn make_partitions(message: message::Message) -> Result<Vec<CreateMessage>, message::MessageError> {
    let message_string: &str = message.to_string();
    if message_string.len() <= DiscordBot::MAX_MESSAGE_LENGTH_ALLOWED {
        Ok(vec![CreateMessage::new().content(message_string)])
    } else {
        let partitions = message.partition_by_text(DiscordBot::MAX_MESSAGE_LENGTH_ALLOWED)?;
        let result = partitions
            .iter()
            .map(|m| CreateMessage::new().content(m.to_string()))
            .collect();

        Ok(result)
    }
}

struct Handler {
    message_tx: mpsc::Sender<message::Message>,
    side: crate::Side,
}

/// Caching for incomming Discord messages.
mod cache {
    use dashmap::DashMap;
    use log::{debug, warn};
    use serenity::{all::CreateMessage, futures::lock::Mutex};
    use std::time::{Duration, Instant};

    use crate::message;

    /// Stale entries are purged after 30 seconds
    pub const MESSAGE_EXPIRATION: Duration = Duration::from_secs(30);

    type MessageParts = Vec<message::Message>;
    type MessageCache = DashMap<u128, (MessageParts, Instant)>;

    lazy_static::lazy_static! {
        pub static ref MESSAGE_CACHE: MessageCache = DashMap::new();
        pub static ref CURRENT_KEY: Mutex<u128> = Mutex::new(0);
        //pub static ref KEY_COUNTER: Mutex<u128> = Mutex::new(0);
    }

    /// Clean up stale entries continually
    pub async fn cleanup_task() {
        debug!("Started cleanup task for message cache");

        tokio::spawn(async move {
            loop {
                // Cleanup every 30 seconds
                tokio::time::sleep(Duration::from_secs(30)).await;

                let now = Instant::now();
                let len_before: usize = MESSAGE_CACHE.len();
                MESSAGE_CACHE.retain(|_, (_, timestamp)| {
                    now.duration_since(*timestamp) < MESSAGE_EXPIRATION
                });

                let len_after: usize = MESSAGE_CACHE.len();

                warn!(
                    "PURGED {} STALE MESSAGES FROM CACHE",
                    len_before - len_after
                );
            }
        });
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: channel::Message) {
        // Exclude messages sent by us
        if msg.author.id == get_bot_id(ctx).await {
            return;
        }

        // Exclude all messages on other guilds
        if msg.guild_id.unwrap_or_default().to_string() != get_discord_guild_id() {
            return;
        }

        let received_message: String = msg.content;
        // Will be sent to the mpsc::Sender

        match message::Message::from_string(&received_message) {
            Ok(message) => {
                let current_side = self.side;
                let message_side = message.direction;

                // Only account the message if its side corresponds with ours.
                if (current_side == Side::Client && message_side == MessageDirection::Clientbound)
                    || (current_side == Side::Server
                        && message_side == MessageDirection::Serverbound)
                {
                    if let Err(err) = self.message_tx.send(message).await {
                        warn!("Failed to enqueue message from Discord: {err}");
                    }
                }
            }
            Err(err) => {
                warn!("Failed to decode message from string: {err}");
            }
        }
    }
}

/// Surely the worst function in this program to have been written
async fn merge_from_cache(
    message: message::Message,
) -> Result<Option<message::Message>, message::MessageError> {
    if message.part.total() == 1 {
        debug!("No need to partition the message. Returned message.");
        return Ok(Some(message));
    }

    let now = Instant::now();
    let messages = (vec![message.clone()], now);
    let mut current_key_guard = cache::CURRENT_KEY.lock().await;

    if cache::MESSAGE_CACHE.is_empty() {
        cache::MESSAGE_CACHE.insert(*current_key_guard, messages);
        debug!("MESSAGE_CACHE was emtpy. Inserted message. Returned message.");
        return Ok(Some(message));
    }

    let cached_messages = cache::MESSAGE_CACHE
        .get(&current_key_guard)
        .ok_or(message::MessageError::Merging("unknown key in cache"))?
        .clone();

    let last_cached_message: &message::Message = cached_messages
        .0
        .last()
        .ok_or(message::MessageError::Merging("expected message, got None"))?;

    // We have different total parts
    if last_cached_message.part.total() != message.part.total() {
        cache::MESSAGE_CACHE.insert(*current_key_guard, messages);
        *current_key_guard += 1;
        error!(
        "Got different total parts between received and cached. Incrementing key, inserting new message into cache. Returned Ok(Some(message))"
            );
        return Ok(Some(message));
    }

    // We have the same total parts
    if last_cached_message.part.total() == message.part.total() {
        // We are the last part
        if last_cached_message.part.current() == message.part.current() - 1 {
            let mut series = cached_messages.0.clone();
            series.push(message);
            *current_key_guard += 1;
            debug!("Finished merging partitions as I got the last part. Returning merged.");
            return Ok(Some(message::Message::merge_partitions(&series)?));
        }

        // We are not the last part. e.g. we are 2/5
        cache::MESSAGE_CACHE.insert(*current_key_guard, messages);
        debug!("We are not the last part. Inserting into cache. Returning Ok(None)");
        return Ok(None);
    }

    error!("Reached end of merge_from_cache(). Returned Ok(None)");
    Ok(None)
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
pub fn get_discord_bot_token(side: crate::Side) -> &'static str {
    let key = if side == crate::Side::Client {
        "CLIENT_DISCORD_BOT_TOKEN"
    } else {
        "SERVER_DISCORD_BOT_TOKEN"
    };

    DISCORD_BOT_TOKEN
        .get_or_init(|| {
            dotenv().ok();
            std::env::var(key).expect("Failed to read DISCORD_BOT_TOKEN from a .env file")
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
