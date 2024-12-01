//! File declaring the Message struct, which represents the data we are sending and receiving
//! in the app.

use std::collections::VecDeque;

use log::debug;
use serenity::futures::AsyncWriteExt;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MessageError {
    #[error("Invalid direction: failed to parse direction from string")]
    InvalidDirection,

    #[error("Hex conversion error: {0}")]
    HexConversionError(String),
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
}

impl TryFrom<&str> for MessageDirection {
    type Error = MessageError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.starts_with(MessageDirection::CLIENTBOUND_HEADER) {
            Ok(MessageDirection::Clientbound)
        } else if value.starts_with(MessageDirection::SERVERBOUND_HEADER) {
            Ok(MessageDirection::Serverbound)
        } else {
            Err(MessageError::InvalidDirection)
        }
    }
}

// Reprensents what is the current part of a Message. 5 out of 10 for example.
#[derive(Clone, Copy, Debug)]
pub struct Part {
    current: usize,
    total: usize,
}

/// Represents a Message in this application.
/// That can be intantiated from a &[u8] or &str.
#[derive(Debug, Clone)]
pub struct Message {
    data: Vec<u8>,
    pub direction: MessageDirection,
    part: Part,
    text_representation: String,
}

impl Message {
    // TODO: MA FLEMME EST INCOMMENSURABLE
    //
    // TODO: CHANGE MESSAGE AND ENCAPSULATE EACH PART OF THE text_representation
    // (HEADER, PART NUMBER, DATA) SO THAT WE CAN SORT AND PARTITION CORRECTLY!!!

    /// Partitions a message given a max number of characters. The text is partitioned based on the
    /// length of its text representation.
    // TODO: INVALID BECAUSE OF text_representation METADATA!!!!!!!!!! (header, ...)
    pub fn partition_by_text(&self, max: usize) -> VecDeque<Self> {
        // Check for invalid `max` values
        if max == 0 {
            panic!("`max` cannot be zero");
        }

        let text_len = self.text_representation.len();
        let whole_parts = text_len / max;
        let remainder = text_len % max;

        let total_parts = if remainder > 0 {
            whole_parts + 1
        } else {
            whole_parts
        };

        let mut queue: VecDeque<Self> = VecDeque::new();

        for i in 0..whole_parts {
            let start = i * max;
            let end = (i + 1) * max;

            let part = &self.text_representation[start..end];
            queue.push_front(Self {
                data: self.data.clone(),
                direction: self.direction,
                part: Part {
                    current: i + 1,
                    total: total_parts,
                },
                text_representation: part.to_string(),
            });
        }

        // Handle any remaining text (final part)
        if remainder > 0 {
            let start = whole_parts * max; // Start of the last part
            let part = &self.text_representation[start..];
            queue.push_front(Self {
                data: self.data.clone(),
                direction: self.direction,
                part: Part {
                    current: total_parts,
                    total: total_parts,
                },
                text_representation: part.to_string(),
            });
        }

        queue
    }

    pub fn merge_partitions(partitions: VecDeque<Self>) -> Self {
        for partition in partitions {
            todo!();
        }

        ()
    }

    fn encode_bytes(data: &[u8]) -> String {
        base85::encode(data)
        //data.iter()
        //    .map(|byte| format!("{byte:02X}"))
        //    .collect::<Vec<String>>()
        //    .join(" ")
    }

    /// Converts a hex string to an array of bytes.
    /// The input string e.g.: "FF 3C A4 52 01 01 02", pairs of digits separated by spaces
    fn decode_string(string: &str) -> Result<Vec<u8>, MessageError> {
        base85::decode(string).map_err(|e| MessageError::HexConversionError(e.to_string()))
        //debug!("In hex_to_bytes(). string={string}");
        //hex::decode(string.replace(" ", ""))
        //    .map_err(|e| MessageError::HexConversionError(e.to_string()))
    }

    // Constructs a Message object from an array of bytes and a direction.
    pub fn from_bytes(data: &[u8], direction: MessageDirection) -> Self {
        let text_representation = format!(
            "{}{}",
            match direction {
                MessageDirection::Clientbound => MessageDirection::CLIENTBOUND_HEADER,
                MessageDirection::Serverbound => MessageDirection::SERVERBOUND_HEADER,
            },
            Message::encode_bytes(data)
        );

        // BEWARE, THE HEX::ENCODE ENCODES ALWAYS TO AN EVEN LENGTH STRING.
        // THE ENCODED STRING WILL BE EXACTLY TWICE AS BIG AS THE NUMBER OF INPUT BYTES.

        Self {
            data: data.to_vec(),
            direction,
            text_representation,
        }
    }

    // Constructs a Message object from a string. Parses the direction from the string.
    pub fn from_string(message: &str) -> Result<Self, MessageError> {
        let direction = MessageDirection::try_from(message)?;

        const CLIENT_HEADER_LEN: usize = MessageDirection::CLIENTBOUND_HEADER.len();
        const SERVER_HEADER_LEN: usize = MessageDirection::SERVERBOUND_HEADER.len();

        // Only take the data after the direction header
        let data: Vec<u8> = match direction {
            MessageDirection::Clientbound => Message::decode_string(&message[CLIENT_HEADER_LEN..])?,
            MessageDirection::Serverbound => Message::decode_string(&message[SERVER_HEADER_LEN..])?,
        };

        debug!("In from_string(). data={data:?}");

        Ok(Self {
            data,
            direction,
            text_representation: message.to_string(),
        })
    }

    // Returns an array of bytes of the Message.
    pub fn to_bytes(&self) -> &[u8] {
        &self.data
    }

    // Returns the string representation from Message.
    // Ready to be sent to Discord.
    pub fn to_string_representation(&self) -> &str {
        &self.text_representation
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_create_message_from_bytes_valid() {
        let data: &[u8] = &[1, 2, 3, 127, 128, 255, 0];
        let direction = MessageDirection::Clientbound;

        let message = Message::from_bytes(data, direction);

        let message_data = &message.data;
        let message_direction = &message.direction;
        // make sure the function does not panic
        let _ = message.to_string_representation();

        assert_eq!(data, message_data);
        assert_eq!(&direction, message_direction,);
    }

    #[test]
    fn test_create_message_from_string_valid() {
        // Test data and expected properties
        let data: &[u8] = &[1, 2, 3, 127, 128, 255, 0];
        let direction = MessageDirection::Serverbound;

        // Convert data to message and then to string
        let message = Message::from_bytes(data, direction);
        let message_string = message.to_string_representation();

        // Convert the string back to a message
        let other_message = Message::from_string(&message_string).unwrap();

        // Validate that data and direction match the original
        assert_eq!(
            data, other_message.data,
            "Data mismatch after round-trip conversion."
        );
        assert_eq!(
            direction, other_message.direction,
            "Direction mismatch after round-trip conversion."
        );

        // Ensure text representation is consistent
        let reconstructed_string = other_message.to_string_representation();
        assert_eq!(
            message_string, reconstructed_string,
            "String representation mismatch after reconstruction."
        );
    }

    #[test]
    fn test_create_message_from_bytes_empty_valid() {
        let data: &[u8] = &[];
        let direction = MessageDirection::Serverbound;

        let message = Message::from_bytes(data, direction);

        assert_eq!(message.data, data);
        assert_eq!(message.direction, direction);
        assert_eq!(
            message.to_string_representation(),
            MessageDirection::SERVERBOUND_HEADER
        );
    }

    #[test]
    fn test_create_message_from_string_empty_valid() {
        let txt = MessageDirection::CLIENTBOUND_HEADER;

        let message = Message::from_string(txt).unwrap();

        assert!(message.data.is_empty());
        assert_eq!(message.direction, MessageDirection::Clientbound);
        assert_eq!(message.text_representation, txt);
    }

    #[test]
    #[should_panic]
    fn test_create_message_from_string_invalid_header() {
        let txt = MessageDirection::SERVERBOUND_HEADER.to_string() + ". FF 00 44 F3 4F AA";
        let _ = Message::from_string(&txt).unwrap();
    }

    #[test]
    #[should_panic]
    fn test_create_message_from_string_invalid_hex() {
        // G is not hex
        let txt = MessageDirection::SERVERBOUND_HEADER.to_string() + "FF 00 44 F3 4F AA 4G";
        let _ = Message::from_string(&txt).unwrap();
    }
}
