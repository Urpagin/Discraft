use std::sync::Arc;

use crate::message;
use log::{debug, error, warn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::Mutex;
use tokio::sync::{broadcast, mpsc};

/// Received TCP packets from a OwnedReadHalf socket and then sends them through a Sender channel.
pub async fn handle_receive_socket(
    socket: OwnedReadHalf,
    tx: mpsc::Sender<message::Message>,
    stop_tx: broadcast::Sender<()>,
    messages_direction: message::MessageDirection,
) {
    let mut stop_rx = stop_tx.subscribe();

    tokio::select! {
        _ = handle_receive_socket_offload(socket, tx, stop_tx, messages_direction) => {
            debug!("Socket receiving handling task finished.");
            return;
        }
        _ = stop_rx.recv() => {
            debug!("Stop signal received. Terminating handler.");
            return;
        }
    }
}

async fn handle_receive_socket_offload(
    mut socket: OwnedReadHalf,
    tx: mpsc::Sender<message::Message>,
    stop_tx: broadcast::Sender<()>,
    messages_direction: message::MessageDirection,
) {
    let mut buffer = [0u8; 2048];

    loop {
        let read: usize = match socket.read(&mut buffer).await {
            Ok(0) => {
                warn!("Socket closed by the peer.");
                stop_tx.send(()).unwrap();
                debug!("Socket error, broadcast stop signal.");
                return;
            }
            Ok(read) => {
                debug!("Received TCP packet [{read}B]: {:?}", &buffer[..read]);
                read
            }
            Err(e) => {
                error!("Failed reading the TCP socket: {e}");
                stop_tx.send(()).unwrap();
                debug!("Socket error, broadcast stop signal.");
                return;
            }
        };

        // Construct the message
        let message = message::Message::from_bytes(&buffer[..read], messages_direction);
        // Send the buffer through a channel.
        if let Err(e) = tx.send(message).await {
            error!("Failed sending message through channel: {e}");
            stop_tx.send(()).unwrap();
            debug!("mpsc channel error, broadcast stop signal");
            return;
        } else {
            debug!("Sent TCP packet message through the mpsc channel");
        }
    }
}

/// Receives messages from a Receiver channel and then sends them through a OwnedWriteHalf TCP socket.
pub async fn handle_channel_to_socket(
    socket: OwnedWriteHalf,
    rx: Arc<Mutex<mpsc::Receiver<message::Message>>>,
    stop_tx: broadcast::Sender<()>,
) {
    let mut stop_rx = stop_tx.subscribe();

    tokio::select! {
        _ = handle_channel_to_socket_offload(socket, rx, stop_tx) => {
            debug!("task finished: handle_channel_to_socket");
            return;
        }
        _ = stop_rx.recv() => {
            debug!("Stop signal received. Terminating handler.");
            return;
        }
    }
}

async fn handle_channel_to_socket_offload(
    mut socket: OwnedWriteHalf,
    rx: Arc<Mutex<mpsc::Receiver<message::Message>>>,
    stop_tx: broadcast::Sender<()>,
) {
    loop {
        match rx.lock().await.recv().await {
            Some(packet) => {
                if let Err(e) = socket.write_all(packet.to_bytes()).await {
                    error!("Failed to send message to socket: {e}");
                    stop_tx.send(()).unwrap();
                    debug!("Failed sending message to socket. Broadcast stop signal");
                    return;
                }
            }
            None => {
                error!("Failed receiving message, channel closed, got None");
                stop_tx.send(()).unwrap();
                debug!("Error receiving message from closed channel (None). Broacast stop signal");
                return;
            }
        }
    }
}
