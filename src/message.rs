//! File declaring the Message struct, which represents the data we are sending and receiving
//! in the app.

use thiserror::Error;

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
    const CLIENTBOUND_HEADER: &'static str = "**Squidward says**:";
    const SERVERBOUND_HEADER: &'static str = "**Cthulhu says**:";

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

/// A module so that we can enforce the use of the new() constructor and check the input values.
pub mod part {
    use crate::message::MessageError;

    /// Reprensents what is the current part of a Message. 5 out of 10 for example.
    #[derive(Clone, Copy, Debug)]
    pub struct Part {
        current: usize,
        total: usize,
    }

    impl Part {
        /// Maximum allowed part number
        pub const UPPER_BOUND: usize = 0xFF;
        /// 2 hex digits + sep '/' + 2 hex digits. e.g.: "0C/0F" has 5 characters.
        pub const PARTITIONING_LENGTH: usize = 5;

        /// Constructor for Part, current and total are checked against the UPPER_BOUND.
        pub fn new(current: usize, total: usize) -> Result<Self, MessageError> {
            if current < 1 {
                Err(MessageError::Partitioning("current cannot be zero"))
            } else if current > Self::UPPER_BOUND {
                Err(MessageError::Partitioning(
                    "current part is greater than the upper bound",
                ))
            } else if total > Self::UPPER_BOUND {
                Err(MessageError::Partitioning(
                    "total part is greater than the upper bound",
                ))
            } else if current > total {
                Err(MessageError::Partitioning(
                    "current part is less then the total part",
                ))
            } else {
                Ok(Self { current, total })
            }
        }

        pub fn current(&self) -> usize {
            self.current
        }

        pub fn total(&self) -> usize {
            self.total
        }

        /// Encodes the partitioning into 2 hex digits.
        /// Max is 0xFF which is 255, and Discord supports messages of 2000 characters.
        /// 2000 * 255 = 510,000 which is larger than the max lenght of a TCP packet (65,535)
        pub fn encode_partitioning(part: Self) -> String {
            format!("{:02X}/{:02X}", part.current(), part.total())
        }

        /// Decodes a partitioning String into a `Part`.
        /// The first character of the string needs to be the beginning of the partitioning,
        /// however, it can be infinitely long.
        pub fn decode_partitioning(text: &str) -> Result<Self, MessageError> {
            // 2 hex digits + sep + 2 hex digits
            if text.len() < Self::PARTITIONING_LENGTH {
                return Err(MessageError::Partitioning(
                    "partitioning string malformed, string smaller than 5 (4 hex digits + sep)",
                ));
            }

            let text = &text[..Self::PARTITIONING_LENGTH];

            let mut tokens = text.split('/');
            let current: usize = tokens
                .next()
                .ok_or(MessageError::Partitioning(
                    "missing current total part of partitioning string",
                ))?
                .parse()
                .map_err(|_| MessageError::Partitioning("failed to parse current into number"))?;

            let total: usize = tokens
                .next()
                .ok_or(MessageError::Partitioning(
                    "missing total part of partitioning string",
                ))?
                .parse()
                .map_err(|_| MessageError::Partitioning("failed to parse total into number"))?;

            // Check if there are extra tokens
            if tokens.next().is_some() {
                return Err(MessageError::Partitioning(
                    "partitioning string contains extra data",
                ));
            }

            Self::new(current, total)
        }
    }
}

/// Represents the text part of a message
#[derive(Clone, Debug)]
pub struct Text {
    /// The whole text ready to be sent
    pub all: String,

    /// e.g.: "**Squidward says**: "
    pub direction: String,

    /// e.g.: "1/2"
    pub partitioning: String,

    /// The encoded bytes
    pub data: String,
}

impl Text {
    pub fn new(direction: MessageDirection, part: part::Part, data: &[u8]) -> Self {
        let direction_text: String = MessageDirection::encode_direction(direction).to_string();
        let partitioning_text: String = part::Part::encode_partitioning(part);
        let data_text: String = Self::encode_data(data);
        let all_text: String = format!("{direction_text}{partitioning_text}{data_text}");

        Self {
            all: all_text,
            direction: direction_text,
            partitioning: partitioning_text,
            data: data_text,
        }
    }

    /// Converts bytes to string representation
    fn encode_data(data: &[u8]) -> String {
        base85::encode(data)
        //data.iter()
        //    .map(|byte| format!("{byte:02X}"))
        //    .collect::<Vec<String>>()
        //    .join(" ")
    }

    /// Converts a string to an array of bytes
    fn decode_data(string: &str) -> Result<Vec<u8>, MessageError> {
        base85::decode(string).map_err(|_| MessageError::Decode("failed to decode base85 string"))
        //debug!("In hex_to_bytes(). string={string}");
        //hex::decode(string.replace(" ", ""))
        //    .map_err(|e| MessageError::HexConversionError(e.to_string()))
    }
}

/// Represents a Message in this application.
/// That can be intantiated from a &[u8] or &str.
#[derive(Debug, Clone)]
pub struct Message {
    data: Vec<u8>,
    pub direction: MessageDirection,
    pub part: part::Part,
    text: Text,
}

impl Message {
    // Constructs a Message object from an array of bytes and a direction.
    pub fn from_bytes(data: &[u8], direction: MessageDirection) -> Self {
        let part = part::Part::new(1, 1).unwrap();
        let text = Text::new(direction, part, data);
        Self {
            data: data.to_vec(),
            direction,
            part,
            text,
        }
    }

    // Constructs a Message object from a string. Parses the direction from the string.
    pub fn from_string(message: &str) -> Result<Self, MessageError> {
        let mut offset: usize = 0;

        let direction = MessageDirection::try_from(message)?;
        offset += MessageDirection::encode_direction(direction).len();

        let part = part::Part::decode_partitioning(&message[offset..])?;
        offset += part::Part::PARTITIONING_LENGTH;

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

    // I could have made partition methods only for `Text`, but oh well, that's the way it is now,
    // I don't want to refactor the code again even if it would make the code simpler and more
    // efficient.
    //
    // TODO: Terrible problem: we are partitioning using the length of the DATA STRING, and not the
    // `full` string!!!
    pub fn partition_by_text(&self, max: usize) -> Result<Vec<Self>, MessageError> {
        // Check for invalid `max` values
        if max == 0 {
            return Err(MessageError::Partitioning(
                "partitioning divisor cannot be zero",
            ));
        }

        // BY TEXT, and not by bytes.
        let data_len: usize = self.text.data.len();
        let whole_parts = data_len / max;
        let remainder = data_len % max;

        let total_parts = if remainder > 0 {
            whole_parts + 1
        } else {
            whole_parts
        };

        let mut queue: Vec<Self> = Vec::new();
        let data_text: &String = &self.text.data;

        for i in 0..whole_parts {
            let start = i * max;
            let end = (i + 1) * max;

            let part_data_string: &str = &data_text[start..end];
            let part_data_bytes: &[u8] = &Text::decode_data(part_data_string)?;
            let direction = self.direction;
            let part = part::Part::new(i + 1, total_parts).expect("Failed to create parts");
            let text = Text::new(direction, part, part_data_bytes);

            queue.push(Self {
                data: part_data_bytes.to_vec(),
                direction,
                part,
                text,
            });
        }

        // Handle any remaining text (final part)
        if remainder > 0 {
            let start = whole_parts * max; // Start of the last part
                                           //
            let part_data_string: &str = &data_text[start..];
            let part_data_bytes: &[u8] = &Text::decode_data(part_data_string)?;
            let direction = self.direction;
            let part = part::Part::new(total_parts, total_parts).expect("Failed to create parts");
            let text = Text::new(direction, part, part_data_bytes);

            queue.push(Self {
                data: part_data_bytes.to_vec(),
                direction,
                part,
                text,
            });
        }

        Ok(queue)
    }

    /// Merges multiple partitions into one `Message`. And concatenates using the
    /// bytes and not string, opposite to the partition_by_text method which partitions by text.
    pub fn merge_partitions(partitions: &[Self]) -> Result<Self, MessageError> {
        let len: usize = partitions.len();

        if len == 0 {
            return Err(MessageError::Merging("there must be at least one element"));
        }

        let mut arr = partitions.to_vec();

        // Descending order selection sort by the part.
        for i in 1..len {
            let mut j = i;
            while j > 0 && arr[j].part.current() < arr[j - 1].part.current() {
                arr.swap(j, j - 1);
                j -= 1;
            }
        }

        // Merged bytes
        let buffer: Vec<u8> = partitions
            .iter()
            .flat_map(|p| p.to_bytes())
            .cloned()
            .collect();

        let direction = partitions[0].direction;
        let part = part::Part::new(1, 1).unwrap();
        let text = Text::new(direction, part, &buffer);

        Ok(Self {
            data: buffer,
            direction,
            part,
            text,
        })
    }

    // Returns the string representation from Message.
    // Ready to be sent to Discord.
    pub fn to_string(&self) -> &str {
        &self.text.all
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
