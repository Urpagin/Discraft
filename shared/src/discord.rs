use std::cell::RefCell;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::rc::Rc;
use std::sync::Arc;

use crate::message;
use log::{error, warn};
use serenity::all::{Channel, ChannelId, CreateChannel, CreateMessage, Http, UserId};
use serenity::async_trait;
use serenity::model::channel;
use serenity::prelude::*;
use tokio::sync::mpsc;
use tokio::sync::OnceCell;

pub struct DiscordBot {
    client: Rc<RefCell<Client>>,
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

        Self {
            client: Rc::new(RefCell::new(client)),
        }
    }

    /// Starts up the bot
    pub async fn start(&self) {
        if let Err(err) = self.client.borrow_mut().start().await {
            error!("Failed to start Discord bot: {err}");
        }
    }

    /// Infinite loop that listens on the receiver and sends the message to Discord channel
    /// as soon as a message is received.
    pub async fn handle_write_discord(
        &self,
        mut rx: mpsc::Receiver<message::Message>,
        channel_ids: &[u64],
    ) {
        let channels = channel_ids
            .iter()
            .map(|id| ChannelId::new(*id))
            .collect::<Vec<ChannelId>>();

        // Clone the Arc<Http> so that we can borrow Http with a ref '&'.
        let http: Arc<Http> = Arc::clone(&self.client.borrow_mut().http);

        // Channel index counter that will rotate.
        // u128 so that we are sure it will never overflow
        let mut counter: u128 = 0;

        // Listen infinitely
        loop {
            match rx.recv().await {
                Some(received_message) => {
                    let rotated_idx: usize = (counter % (channels.len() - 1) as u128) as usize;
                    counter += 1;

                    let channel = channels[rotated_idx];
                    let message_content = received_message.to_string_representation();
                    let message = CreateMessage::new().content(message_content);

                    if let Err(err) = channel.send_message(&http, message).await {
                        warn!("Failed to send message to Discord channel: {err}");
                    }
                }
                None => {
                    error!("Received None (channel closed): exiting the function");
                    return;
                }
            }
        }
    }
}

struct Handler {
    message_tx: mpsc::Sender<message::Message>,
}

/// A lazy-initialized value because in the handler, we need the value of the botID to ignore our
/// messages, so we'll query it once and reuse it for the rest of the program's lifetime.
static BOT_ID: OnceCell<UserId> = OnceCell::const_new();

async fn get_bot_id(ctx: Context) -> UserId {
    // Initialize the value if not already initialized
    *BOT_ID
        .get_or_init(|| async { ctx.http.get_current_user().await.unwrap().id })
        .await
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: channel::Message) {
        // Exclude messages sent by us
        if msg.author.id == get_bot_id(ctx).await {
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
