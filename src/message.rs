//! File declaring the Message struct, which represents the data we are sending and receiving
//! in the app.

use std::fmt::Debug;

use thiserror::Error;

use crate::partitioning::{self, Aggregator, Part};

#[derive(Debug, Error)]
pub enum MessageError {
    #[error("Invalid direction: {0}")]
    Direction(&'static str),

    #[error("Conversion error: {0}")]
    Decode(&'static str),

    #[error("Invalid partitioning: {0}")]
    Partitioning(&'static str),

    #[error("Invalid aggregation: {0}")]
    Aggregation(&'static str),

    #[error("Merging error: {0}")]
    Merging(&'static str),
}

/// An attribute specifying who should account for the packet.
///
/// - "CLIENTBOUND" is towards the client, the client should read it.
/// - "SERVERBOUND" is towards the server, the server should read it.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum MessageDirection {
    Clientbound,
    Serverbound,
}

impl MessageDirection {
    const CLIENTBOUND_HEADER: &'static str = "**Squidward says**: ";
    const SERVERBOUND_HEADER: &'static str = "**Cthulhu says**: ";

    /// Encodes the direction to String
    pub fn to_string(self) -> &'static str {
        match self {
            MessageDirection::Clientbound => MessageDirection::CLIENTBOUND_HEADER,
            MessageDirection::Serverbound => MessageDirection::SERVERBOUND_HEADER,
        }
    }

    /// Decodes the first direction from text
    pub fn from_string(text: &str) -> Result<MessageDirection, MessageError> {
        MessageDirection::try_from(text)
    }
}

impl TryFrom<&str> for MessageDirection {
    type Error = MessageError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.starts_with(MessageDirection::CLIENTBOUND_HEADER) {
            Ok(MessageDirection::Clientbound)
        } else if value.starts_with(MessageDirection::SERVERBOUND_HEADER) {
            Ok(MessageDirection::Serverbound)
        } else {
            Err(MessageError::Direction("unknown direction header"))
        }
    }
}

/// Represents a Message in this application.
/// That can be intantiated from strings and bytes.
/// Message layout [length, direction, part, payload]
///
/// # Length
///
/// The length is a String at the beginning of each message in the format:
/// "<length of the header + payload except for the length itsef in decimal><delimiter>"
#[derive(Debug, Clone)]
pub struct Message {
    // The length of the header(except length itself) + payload.
    pub length: String,
    // Either clientbound, or serverbound.
    pub direction: MessageDirection,
    // X/Y to partition messages into smaller ones. (e.g. 2/5)
    pub part: partitioning::Part,

    // The actual bytes of data. The payload.
    payload: Vec<u8>,

    // The full message as a String. Ready to be sent to Discord.
    text: String,
}

impl Message {
    pub const LENGTH_DELIMITER: char = '*';
    pub const HALT_MESSAGE: &'static str =
        "BY THE GRACE OF GOD, I HEREBY COMMAND YOU TO KILL YOUSELF NOW!";

    /// Returs either true of false the input message is a halt message.
    pub fn is_halt_message(message: &Message) -> bool {
        let payload_text: String = Self::payload_bytes_to_string(message.payload());
        if payload_text == Self::HALT_MESSAGE {
            true
        } else {
            false
        }
    }

    /// Returns a standart halt message.
    pub fn make_halt_message(direction: MessageDirection) -> Self {
        let part = Part::new(1, 1).unwrap();
        let message = Self::make_string(
            &direction,
            &part,
            &Self::payload_string_to_bytes(Self::HALT_MESSAGE)
                .expect("Failed to make halt message. (I)"),
        );

        Self::from_string(message.0 + &message.1)
            .expect("Failed to make halt message. (II)")
            .iter()
            .next()
            .expect("Failed to make halt message. (III)")
            .clone()
    }

    // Constructs a Message object from an array of bytes and a direction.
    pub fn from_bytes<T: AsRef<[u8]>>(data: T, direction: MessageDirection) -> Self {
        let data: &[u8] = data.as_ref();
        let part = Part::new(1, 1).unwrap();

        let (length, text) = Self::make_string(&direction, &part, data);

        Self {
            length,
            direction,
            part,
            payload: data.to_vec(),
            text,
        }
    }

    // Constructs a Message object from a string.
    // Parses the direction from the string.
    //
    // Can return multiple messages if the string is an aggregate of messages.
    pub fn from_string<T: AsRef<str>>(message: T) -> Result<Vec<Self>, MessageError> {
        // Use the parsing function from Aggregator.
        Aggregator::disaggregate(message.as_ref())
    }

    // Returns an array of bytes of the Message.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Converts bytes to string representation
    pub fn payload_bytes_to_string(data: &[u8]) -> String {
        base85::encode(data)
        //data.iter()
        //    .map(|byte| format!("{byte:02X}"))
        //    .collect::<Vec<String>>()
        //    .join(" ")
    }

    /// Converts a string to an array of bytes
    pub fn payload_string_to_bytes(string: &str) -> Result<Vec<u8>, MessageError> {
        base85::decode(string).map_err(|_| MessageError::Decode("Failed to decode base85 string"))
        //debug!("In hex_to_bytes(). string={string}");
        //hex::decode(string.replace(" ", ""))
        //    .map_err(|e| MessageError::HexConversionError(e.to_string()))
    }

    /// Makes the string representation of the message.
    ///
    /// # Returns
    ///
    /// A tuple (Length, Message(except String))
    ///
    /// So to build the complete packet, just flatten the tuple into a String, and send it's ready
    /// to be sent to Discord.
    pub fn make_string(
        direction: &MessageDirection,
        part: &Part,
        payload: &[u8],
    ) -> (String, String) {
        let mut message_str_except_length = String::with_capacity(100);
        message_str_except_length.push_str(direction.to_string());
        message_str_except_length.push_str(&part.to_string());
        message_str_except_length.push_str(&Self::payload_bytes_to_string(payload));

        // With length excluded.
        let length: usize = message_str_except_length.len();
        (
            // Length string
            length.to_string() + &Self::LENGTH_DELIMITER.to_string(),
            // Rest of the message string
            message_str_except_length,
        )
    }

    // Returns the string representation from Message.
    // Ready to be sent to Discord.
    pub fn to_string(&self) -> &str {
        &self.text
    }
}

// TODO: WRITE THOROUGH TESTS!
