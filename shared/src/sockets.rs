use crate::message;
use log::{debug, error, warn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc;

/// Received TCP packets from a OwnedReadHalf socket and then sends them through a Sender channel.
pub async fn handle_receive_socket(
    mut socket: OwnedReadHalf,
    tx: mpsc::Sender<message::Message>,
    messages_direction: message::MessageDirection,
) {
    let mut buffer = [0u8; 2048];
    loop {
        // Read socket for new packets and buffer it.
        let read: usize = match socket.read(&mut buffer).await {
            Ok(0) => {
                warn!("Socket closed by the peer.");
                break;
            }
            Ok(read) => {
                debug!("Received TCP packet [{read}B]: {:?}", &buffer[..read]);
                read
            }
            Err(e) => {
                error!("Failed reading the TCP socket: {e}");
                break;
            }
        };

        // Construct the message
        let message = message::Message::from_bytes(&buffer[..read], messages_direction);
        // Send the buffer through a channel.
        if let Err(e) = tx.send(message).await {
            error!("Failed sending message through channel: {e}");
            break;
        }
    }
}

/// Receives messages from a Receiver channel and then sends them through a OwnedWriteHalf TCP socket.
pub async fn handle_channel_to_socket(
    mut socket: OwnedWriteHalf,
    mut rx: mpsc::Receiver<message::Message>,
) {
    loop {
        match rx.recv().await {
            Some(packet) => {
                if let Err(e) = socket.write_all(packet.to_bytes()).await {
                    error!("Failed to send message to socket: {e}");
                    break;
                }
            }
            None => {
                error!("Failed receiving message, channel closed, got None");
                break;
            }
        }
    }
}
