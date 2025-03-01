use std::sync::Arc;

use crate::message;
use log::{debug, error, warn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::Mutex;
use tokio::sync::{broadcast, mpsc};

/// Sends a Discord halt message if a stop signal is received.
async fn stop_signal_listener(
    stop_tx: broadcast::Sender<()>,
    tx: mpsc::Sender<message::Message>,
    message_direction: message::MessageDirection,
) {
    let mut stop_rx = stop_tx.subscribe();
    tokio::spawn(async move {
        loop {
            if let Err(err) = stop_rx.recv().await {
                warn!("Failed to receive from stop signal tx: {err}");
            } else {
                let halt_message = message::Message::make_halt_message(message_direction);
                if let Err(err) = tx.send(halt_message).await {
                    warn!("Failed to send halt message to tx: {err}");
                }
            }
        }
    });
}

/// Received TCP packets from a OwnedReadHalf socket and then sends them through a Sender channel.
pub async fn handle_receive_socket(
    socket: OwnedReadHalf,
    tx: mpsc::Sender<message::Message>,
    stop_tx: broadcast::Sender<()>,
    messages_direction: message::MessageDirection,
) {
    let mut stop_rx = stop_tx.subscribe();

    // Sends a Discord halt message if stop signal received.
    stop_signal_listener(stop_tx.clone(), tx.clone(), messages_direction).await;

    tokio::select! {
        _ = handle_receive_socket_offload(socket, tx, stop_tx, messages_direction) => { debug!("Socket receiving handling task finished.") }
        _ = stop_rx.recv() => {
            debug!("Stop signal received. Terminating handler.");
            return;
            //info!("SENT DISCORD HALT MESSAGE");
        }
    }
}

async fn handle_receive_socket_offload(
    mut socket: OwnedReadHalf,
    tx: mpsc::Sender<message::Message>,
    stop_tx: broadcast::Sender<()>,
    messages_direction: message::MessageDirection,
) {
    // Pre-allocate some buffer space to reduce allocations.
    let mut buffer = Vec::with_capacity(8192);

    loop {
        // Loop that read all the data on the socket
        match socket.read_buf(&mut buffer).await {
            Ok(0) => {
                warn!("Socket closed by the peer.");
                let _ = stop_tx.send(());
                debug!("Socket error, broadcast stop signal.");
                return;
            }
            Ok(read) => {
                debug!("Received TCP packet from MINECRAFT [{read}B]");
            }
            Err(e) => {
                error!("Failed reading the TCP socket: {e}");
                let _ = stop_tx.send(());
                debug!("Socket error, broadcast stop signal.");
                return;
            }
        }

        // Construct the message
        let message = message::Message::from_bytes(&buffer, messages_direction);

        // Send the buffer through the channel
        if let Err(e) = tx.send(message).await {
            error!("Failed sending message through channel: {e}");
            let _ = stop_tx.send(());
            debug!("mpsc channel error, broadcast stop signal");
            return;
        } else {
            debug!("Sent TCP packet message through the mpsc channel");
        }

        // Clear buffer for the next read cycle
        buffer.clear();
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
        _ = handle_channel_to_socket_offload(socket, rx, stop_tx) => { debug!("task finished: handle_channel_to_socket") }
        _ = stop_rx.recv() => { debug!("Stop signal received. Terminating handler.") }
    }
}

async fn handle_channel_to_socket_offload(
    mut socket: OwnedWriteHalf,
    rx: Arc<Mutex<mpsc::Receiver<message::Message>>>,
    stop_tx: broadcast::Sender<()>,
) {
    debug!("Inside handle_channel_to_socket_offload");

    loop {
        let packet = {
            debug!("Getting the mutex guard");
            let mut rx_guard = rx.lock().await;
            debug!("Acquired the mutex guard");
            rx_guard.recv().await
        };

        match packet {
            Some(packet) => {
                if let Err(e) = socket.write_all(packet.payload()).await {
                    error!("Failed to send message to socket: {e}");
                    stop_tx.send(()).unwrap();
                    debug!("Failed sending message to socket. Broadcast stop signal");
                    return;
                } else {
                    debug!("Sent packet to MC");
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
