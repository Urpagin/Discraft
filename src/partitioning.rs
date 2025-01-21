use once_cell::sync::Lazy;

use crate::{
    discord::DiscordBot,
    message::{MessageDirection, MessageError},
};

use super::message::Message;

/// Represents the text part of a message
#[derive(Clone, Debug)]
pub struct TextMessage {
    /// e.g.: "**Squidward says**: "
    pub direction: String,

    /// e.g.: "1/2"
    pub partitioning: String,

    /// The actual data bytes of the packet but as a string
    pub data: String,

    /// The whole text ready to be sent
    ///
    /// direction + partitioning + data
    pub message: String,
}

impl TextMessage {
    pub fn new<T: AsRef<[u8]>>(direction: MessageDirection, part: Part, data: T) -> Self {
        let data: &[u8] = data.as_ref();

        let direction_text: String = MessageDirection::encode_direction(direction).to_string();
        let partitioning_text: String = part.to_string();
        let data_text: String = Self::encode_data(data);

        let message_text: String = format!("{direction_text}{partitioning_text}{data_text}");

        Self {
            direction: direction_text,
            partitioning: partitioning_text,
            data: data_text,

            message: message_text,
        }
    }

    pub fn from_message(message: &Message) -> Self {
        TextMessage::new(message.direction, message.part, message.to_bytes())
    }

    pub fn to_message(&self) -> Message {
        // .unwrap() because self is valid.
        Message::from_string(&self.message).unwrap()
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
        base85::decode(string).map_err(|_| MessageError::Decode("Failed to decode base85 string"))
        //debug!("In hex_to_bytes(). string={string}");
        //hex::decode(string.replace(" ", ""))
        //    .map_err(|e| MessageError::HexConversionError(e.to_string()))
    }
}

// impl From<TextMessage> for Message
// is also defined.
impl From<Message> for TextMessage {
    fn from(value: Message) -> Self {
        Self::from_message(&value)
    }
}

/// Represents a message that has been partitioned into multiple other parts.
///
/// # Usage
///
/// Initialize the object with the `partition()` function.
/// todo
/// let partitioned = PartitionedMessage::partition(...)
// TODO: Rewrite the docstring
pub struct PartitionedMessage {
    parts: Vec<Message>,
}

impl PartitionedMessage {
    /// Takes in multiple `Message` and returns an object of Self.
    fn new<T: AsRef<[Message]>>(parts: T) -> Self {
        Self {
            parts: parts.as_ref().to_vec(),
        }
    }

    /// This function partitions by text, and not bytewise!
    ///
    /// IMPORTANT: Everything might just blow up if the message encoding is done with UTF-8 characters.
    pub fn partition(message: Message, str_len_limit: usize) -> Result<Self, MessageError> {
        // Check for invalid `max` values
        if str_len_limit == 0 {
            return Err(MessageError::Partitioning(
                "partitioning divisor cannot be zero",
            ));
        }

        let direction: &String = &message.text.direction;
        let payload: String = message.text.data;
        let payload_len: usize = payload.len();

        // Size of the payload (STRING)
        let header_size: usize = direction.len() + Part::get_standard_string_length();
        if str_len_limit <= header_size {
            return Err(MessageError::Partitioning(
                "length limit is too small to accommodate the header",
            ));
        }
        // The number of payload characters we can put while still being able to put the header.
        let payload_slice_size: usize = str_len_limit - header_size;

        // Compute the number of partitions we will need to create
        let whole_parts: usize = payload_len / payload_slice_size;
        let remainder: usize = payload_len % payload_slice_size;
        let total_parts = if remainder > 0 {
            whole_parts + 1
        } else {
            whole_parts
        };

        // The number of payload characters we have partitioned
        let mut put_payload_chars: usize = 0;
        // The current part number. Like 1/2 (current/total).
        let mut current_part: usize = 1;
        // All the parts that make up the inputted message
        let mut parts: Vec<Message> = Vec::with_capacity(total_parts);

        // Where the partition will be stored each loop iteration
        let mut part_buffer: String = String::with_capacity(str_len_limit);

        // Exits when all the payload has been partitioned
        // Also, we have computted the number of parts, surely there's a way to not use a while
        // loop.
        while put_payload_chars < payload.len() {
            let part: String = Part::new(current_part, total_parts)?.to_string();
            current_part += 1;

            let start: usize = put_payload_chars;
            let stop = usize::min(start + payload_slice_size, payload_len); // Prevent out-of-bounds slicing
            let sliced_payload: &str = &payload[start..stop];
            put_payload_chars = stop; // Update position

            // Construct the full partition string
            part_buffer.clear();
            part_buffer.push_str(&direction);
            part_buffer.push_str(&part);
            part_buffer.push_str(sliced_payload);
            let part = Message::from_string(&part_buffer)?;
            parts.push(part);
        }

        Ok(Self { parts })
    }

    /// Merges all the `Message`s in the current `PartitionedMessage` object and tries to return a
    /// `Message`.
    pub fn merge(&self) -> Result<Message, MessageError> {
        // Handle case where there are no parts
        if self.parts.is_empty() {
            return Err(MessageError::Partitioning("No parts to merge"));
        }

        // Extract direction from the first part
        let direction = self.parts[0].direction;

        let max_message_length: usize = self.parts.len() * DiscordBot::MAX_MESSAGE_LENGTH_ALLOWED;
        let mut payload_buffer: Vec<u8> = Vec::with_capacity(max_message_length);

        // Merge all parts
        for part in &self.parts {
            payload_buffer.extend_from_slice(part.to_bytes());
        }

        // Create and return the merged Message
        Ok(Message::from_bytes(payload_buffer, direction))
    }
}

/// Reprensents what is the current part of a Message. 5 out of 10 for example.
/// Represents the positioning of a Message in a sequence of partitioned messages.
///
/// With the `current` and `total` fields, for example, "2 out of 8".
#[derive(Clone, Copy, Debug)]
pub struct Part {
    current: usize,
    total: usize,
}

impl Part {
    /// Maximum allowed part number (255)
    pub const MAX_TOTAL: usize = 0xFF;

    /// Constructs a valid `Part` given the `current` and `total` arguments.
    /// A `Part`'s `total` cannot be greater than `MAX_TOTAL`.
    pub fn new(current: usize, total: usize) -> Result<Self, MessageError> {
        // reminder: usize cannot be negative, no need to check
        if current == 0 || current > total {
            Err(MessageError::Partitioning(
                "current must be between 1 and total (inclusive).",
            ))
        } else if total > Self::MAX_TOTAL {
            Err(MessageError::Partitioning("total cannot exceed MAX_TOTAL."))
        } else {
            Ok(Self { current, total })
        }
    }

    /// Returns a copy of `current`.
    pub fn current(&self) -> usize {
        self.current
    }

    /// Returns a copy of `total`.
    pub fn total(&self) -> usize {
        self.total
    }

    /// Encodes the partitioning into 2 hex digits.
    /// Max is 0xFF which is 255, and Discord supports messages of 2000 characters.
    /// 2000 * 255 = 510,000 which is larger than the max lenght of a TCP packet (65,535)
    pub fn to_string(&self) -> String {
        format!("{:02X}/{:02X} ", self.current, self.total)
    }

    /// Decodes a partitioning string into a `Part`.
    /// The first section of the string must represent the partitioning format (`current/total`),
    /// and additional content is disallowed.
    ///
    /// Example:
    /// "01/10" -> Part { current: 1, total: 10 }
    pub fn from_string<T: AsRef<str>>(text: T) -> Result<Self, MessageError> {
        let text: &str = text.as_ref();

        // Check if the string is long enough
        let expected_len = Self::get_standard_string_length();
        if text.len() < expected_len {
            return Err(MessageError::Partitioning(
                "Partitioning string malformed: string too small",
            ));
        }

        // Slice to the expected length
        let text = &text[..expected_len];
        let mut tokens = text.split('/');

        // Parse current value
        let current: usize = tokens
            .next()
            .ok_or_else(|| {
                MessageError::Partitioning("Missing 'current' part in partitioning string")
            })?
            .parse()
            .map_err(|_| MessageError::Partitioning("Failed to parse 'current' as a number"))?;

        // Parse total value
        let total: usize = tokens
            .next()
            .ok_or_else(|| {
                MessageError::Partitioning("Missing 'total' part in partitioning string")
            })?
            .parse()
            .map_err(|_| MessageError::Partitioning("Failed to parse 'total' as a number"))?;

        // Ensure no extra tokens exist
        if tokens.next().is_some() {
            return Err(MessageError::Partitioning(
                "Partitioning string contains unexpected extra data",
            ));
        }

        // Construct the part
        Self::new(current, total)
    }

    /// Returns the length of the encoded (to String) `Part`.
    /// So if (current=1, total=1). The encoded String will be '01/01'
    /// and this function will return 5.
    pub fn get_standard_string_length() -> usize {
        // Compute the value once
        // Dummy part 1/1
        static STANDARD_STRING_LENGTH: Lazy<usize> =
            Lazy::new(|| Part::new(1, 1).unwrap().to_string().len());

        // Return the cached value
        *STANDARD_STRING_LENGTH
    }
}
