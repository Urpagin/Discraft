//! File declaring the Message struct, which represents the data we are sending and receiving
//! in the app.

use std::fmt::Debug;

use thiserror::Error;

use crate::partitioning::{self, Part, TextMessage};

#[derive(Debug, Error)]
pub enum MessageError {
    #[error("Invalid direction: {0}")]
    Direction(&'static str),

    #[error("Conversion error: {0}")]
    Decode(&'static str),

    #[error("Invalid partitioning: {0}")]
    Partitioning(&'static str),

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
    pub fn encode_direction(direction: MessageDirection) -> &'static str {
        match direction {
            MessageDirection::Clientbound => MessageDirection::CLIENTBOUND_HEADER,
            MessageDirection::Serverbound => MessageDirection::SERVERBOUND_HEADER,
        }
    }

    /// Decodes the direction from text
    pub fn decode_direction(text: &str) -> Result<MessageDirection, MessageError> {
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
/// That can be intantiated from a &[u8] or &str.
#[derive(Debug, Clone)]
pub struct Message {
    data: Vec<u8>,
    pub direction: MessageDirection,
    pub part: partitioning::Part,
    pub text: partitioning::TextMessage,
}

const HALT_MESSAGE: &'static str = "OI! OI! OI! KYS NOW!";
const HALT_MESSAGE_BYTES: &[u8] = HALT_MESSAGE.as_bytes();

impl Message {
    /// A message that should halt everything if read
    pub fn make_halt_message(direction: MessageDirection) -> Self {
        let data: Vec<u8> = HALT_MESSAGE_BYTES.to_vec();
        let part = part::Part::new(1, 1).unwrap();
        let text = Text::new(direction, part, &data);
        Self {
            data,
            direction,
            text,
        }
    }

    /// Determins if a message is a halt message
    pub fn is_halt_message(message: &Self) -> bool {
        message.data == HALT_MESSAGE_BYTES
    }

    // Constructs a Message object from an array of bytes and a direction.
    pub fn from_bytes<T: AsRef<[u8]>>(data: T, direction: MessageDirection) -> Self {
        let data: &[u8] = data.as_ref();
        let part = Part::new(1, 1).unwrap();
        let text = TextMessage::new(direction, part, data);
        Self {
            data: data.to_vec(),
            direction,
            part,
            text,
        }
    }

    // Constructs a Message object from a string.
    // Parses the direction from the string.
    pub fn from_string(message: &str) -> Result<Self, MessageError> {
        let mut offset: usize = 0;

        let direction = MessageDirection::try_from(message)?;
        offset += MessageDirection::encode_direction(direction).len();

        let part = part::Part::decode_partitioning(&message[offset..])?;
        offset += part::Part::get_partitioning_length();

        let data = Text::decode_data(&message[offset..])?;

        let text = Text::new(direction, part, &data);

        Ok(Self {
            data,
            direction,
            part,
            text,
        })
    }

    // Returns an array of bytes of the Message.
    pub fn to_bytes(&self) -> &[u8] {
        &self.data
    }

    // Returns the string representation from Message.
    // Ready to be sent to Discord.
    pub fn to_string(&self) -> &str {
        &self.text.message
    }
}

impl From<TextMessage> for Message {
    fn from(value: TextMessage) -> Self {
        value.to_message()
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
        let _ = message.to_string();

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
        let message_string = message.to_string();

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
        let reconstructed_string = other_message.to_string();
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

        let part: String = part::Part::encode_partitioning(part::Part::new(1, 1).unwrap());

        assert_eq!(message.text.direction, MessageDirection::SERVERBOUND_HEADER);
        assert_eq!(message.text.partitioning, part);
        assert_eq!(message.text.data, "");
        assert_eq!(
            message.text.all,
            format!(
                "{}{}{}",
                message.text.direction, message.text.partitioning, message.text.data
            )
        );
    }

    #[test]
    fn test_create_message_from_string_empty_valid() {
        let part = part::Part::new(1, 1).unwrap();
        let txt: String = MessageDirection::CLIENTBOUND_HEADER.to_string()
            + &part::Part::encode_partitioning(part);

        let message = Message::from_string(&txt).unwrap();

        assert!(message.data.is_empty());
        assert!(message.text.data.is_empty());
        assert_eq!(message.direction, MessageDirection::Clientbound);
        assert_eq!(message.to_string(), txt);
    }

    #[test]
    #[should_panic]
    fn test_create_message_from_string_invalid_header() {
        let txt = MessageDirection::SERVERBOUND_HEADER.to_string() + "qlsdjk flqs dkf23 9483";
        let _ = Message::from_string(&txt).unwrap();
    }

    #[test]
    #[should_panic]
    fn test_create_message_from_string_invalid_encoding() {
        // こんにちは inside
        let txt = MessageDirection::SERVERBOUND_HEADER.to_string() + "87cURD_こんにちは*#4DfTZ)+T";
        let _ = Message::from_string(&txt).unwrap();
    }

    #[test]
    fn test_create_partitions_valid() {
        let data = &[
            1, 44, 55, 100, 0, 255, 127, 4, 5, 6, 2, 8, 88, 99, 11, 12, 0, 1, 4,
        ];
        let direction = MessageDirection::Serverbound;

        let message = Message::from_bytes(data, direction);

        const DIVISOR: usize = 2;
        let parts = message.partition_by_text(DIVISOR).unwrap();

        // in the partitioninig function we use the len of the data string :/, not `full` :(
        let text_len: usize = message.text.data.len();

        let parts_number_whole = text_len / DIVISOR;
        let parts_remainder = text_len % DIVISOR;
        let parts_total = if parts_remainder > 0 {
            parts_number_whole + 1
        } else {
            parts_number_whole
        };

        for i in 0..parts.len() - 1 {
            let current = parts[i].clone();
            let next = parts[i + 1].clone();
            assert!(current.part.current() < next.part.current());
        }

        assert_eq!(parts.len(), parts_total);
    }
}
