//! File declaring the Message struct, which represents the data we are sending and receiving
//! in the app.

use std::fmt::Debug;

use base64::{engine::general_purpose, Engine};
use lazy_static::lazy_static;
use once_cell::sync::Lazy;
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

const HALT_DATA: &[u8; 8] = &[3, 4, 4, 0, 1, 1, 1, 1];
pub static HALT_MESSAGE_DECODED: Lazy<String> = Lazy::new(|| base85::encode(&HALT_DATA.to_vec()));

impl Message {
    pub const LENGTH_DELIMITER: char = '~';
    /// Returs either true of false the input message is a halt message.
    pub fn is_halt_message(message: &Message) -> bool {
        let payload_text: String = Self::payload_bytes_to_string(message.payload());
        if payload_text == *HALT_MESSAGE_DECODED {
            true
        } else {
            false
        }
    }

    /// Returns a standart halt message.
    pub fn make_halt_message(direction: MessageDirection) -> Self {
        let part = Part::new(1, 1).unwrap();
        let message = Self::make_string(&direction, &part, HALT_DATA);

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
            length: length.clone(),
            direction,
            part,
            payload: data.to_vec(),
            text: length + &text,
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
        println!("payload_bytes_to_string() input: {data:?}");
        //base85::encode(data)
        //general_purpose::STANDARD.encode(data)
        // base64::Engine::encode(&self, input)
        data.iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<String>>()
            .join(" ")
    }

    /// Converts a string to an array of bytes
    pub fn payload_string_to_bytes(string: &str) -> Result<Vec<u8>, MessageError> {
        //base85::decode(string).map_err(|_| MessageError::Decode("Failed to decode base85 string"))
        // general_purpose::STANDARD
        //     .decode(string)
        //     .map_err(|_| MessageError::Decode("Failed to decode base85 string"))

        //debug!("In hex_to_bytes(). string={string}");
        hex::decode(string.replace(" ", ""))
            .map_err(|e| MessageError::Decode("failed to decode hex"))
    }

    /// Makes the string representation of the message.
    ///
    /// # Returns
    ///
    /// a tuple (length, message(except String))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::partitioning::{Aggregator, Part};

    #[test]
    fn test_message_direction_to_string() {
        // Test that each MessageDirection returns the correct header.
        assert_eq!(
            MessageDirection::Clientbound.to_string(),
            "**Squidward says**: "
        );
        assert_eq!(
            MessageDirection::Serverbound.to_string(),
            "**Cthulhu says**: "
        );
    }

    #[test]
    fn test_message_direction_from_string() {
        // Create strings starting with valid headers.
        let client_str = format!("{}some payload", MessageDirection::Clientbound.to_string());
        let server_str = format!("{}some payload", MessageDirection::Serverbound.to_string());

        // Check correct parsing.
        assert_eq!(
            MessageDirection::from_string(&client_str).unwrap(),
            MessageDirection::Clientbound
        );
        assert_eq!(
            MessageDirection::from_string(&server_str).unwrap(),
            MessageDirection::Serverbound
        );

        // Test with an invalid header.
        let invalid_str = "Invalid header: payload";
        assert!(MessageDirection::from_string(invalid_str).is_err());
    }

    #[test]
    fn test_try_from_for_message_direction() {
        // Verify that TryFrom<&str> works as expected.
        let s = format!("{}data", MessageDirection::Clientbound.to_string());
        let result = MessageDirection::try_from(s.as_str());
        assert_eq!(result.unwrap(), MessageDirection::Clientbound);
    }

    #[test]
    fn test_payload_conversion() {
        // Test that encoding and then decoding recovers the original bytes.
        let original_bytes = vec![104, 101, 108, 108, 111]; // "hello"
        let encoded = Message::payload_bytes_to_string(&original_bytes);
        let decoded = Message::payload_string_to_bytes(&encoded).unwrap();
        assert_eq!(original_bytes, decoded);
    }

    #[test]
    fn test_make_string_length() {
        let direction = MessageDirection::Clientbound;
        let part = Part::new(1, 1).unwrap();
        let payload = b"test payload";
        let (length_str, msg_body) = Message::make_string(&direction, &part, payload);

        // The length string should contain the length and the delimiter.
        let mut parts = length_str.split(Message::LENGTH_DELIMITER);
        let len_number_str = parts.next().unwrap();
        let len_number = len_number_str.parse::<usize>().unwrap();

        // Ensure the reported length matches the message body length.
        assert_eq!(len_number, msg_body.len());

        // The message body should begin with the header and contain the partition and encoded payload.
        assert!(msg_body.starts_with(MessageDirection::Clientbound.to_string()));
        assert!(msg_body.contains(&part.to_string()));
        let encoded_payload = Message::payload_bytes_to_string(payload);
        assert!(msg_body.contains(&encoded_payload));
    }

    #[test]
    fn test_from_bytes() {
        let direction = MessageDirection::Serverbound;
        let payload = b"sample payload";
        let message = Message::from_bytes(payload, direction);

        // Verify the direction and payload.
        assert_eq!(message.direction, direction);
        assert_eq!(message.payload(), payload);

        // Ensure the complete message string contains the proper header and encoded payload.
        let header = direction.to_string();
        assert!(message.text.contains(header));
        let encoded_payload = Message::payload_bytes_to_string(payload);
        assert!(message.text.contains(&encoded_payload));
    }

    #[test]
    fn test_halt_message() {
        let halt_msg = Message::make_halt_message(MessageDirection::Clientbound);

        // Check that the halt message is recognized.
        assert!(Message::is_halt_message(&halt_msg));

        // Verify that decoding the payload recovers the halt message string.
        let payload_decoded = Message::payload_bytes_to_string(halt_msg.payload());
        assert_eq!(payload_decoded, *HALT_MESSAGE_DECODED);
    }

    #[test]
    fn test_from_string_aggregation() {
        // Construct a valid message string using make_string.
        let direction = MessageDirection::Clientbound;
        let part = Part::new(1, 1).unwrap();
        let payload = b"aggregated message";
        let (length_str, msg_body) = Message::make_string(&direction, &part, payload);
        let full_message = format!("{}{}", length_str, msg_body);

        // Use the Aggregator to disaggregate the message.
        let messages = Message::from_string(full_message).unwrap();
        assert_eq!(messages.len(), 1);

        // Validate that the parsed message has the expected direction and payload.
        let message = &messages[0];
        assert_eq!(message.direction, direction);
        assert_eq!(message.payload(), payload);
    }
}
