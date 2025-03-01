//! Everything to partition and aggregate `Message`s.
//!
//! In this project's context:
//!
//! - paritioning is taking a "big" `Message` and transforming it into multiple smaller `Message`s.
//!
//! - aggregation is taking multiple "small" `Message`s and transformaing them into a single, or
//!   multiple, "big" compound `AggregateMessage`.

use once_cell::sync::Lazy;

use crate::{
    discord::DiscordBot,
    message::{Message, MessageDirection, MessageError},
};

// Functions to partition and merge `Message`s.
pub struct Partitioner {}

impl Partitioner {
    /// This function partitions BY TEXT, and not bytewise!
    /// Takes a message and returns smaller messages that all fit within the character limit.
    ///
    /// If the input message is already smaller than the max chars, it is returned.
    ///
    /// # ! IMPORTANT !
    ///
    /// IMPORTANT!!: Everything might just blow up if the message encoding is done with UTF-8 characters (non-ASCII).
    pub fn partition(message: Message, str_len_limit: usize) -> Result<Vec<Message>, MessageError> {
        // Check for invalid `max` values
        if str_len_limit == 0 {
            return Err(MessageError::Partitioning(
                "partitioning divisor cannot be zero",
            ));
        }

        let length: &String = &message.length;
        let direction: &str = message.direction.to_string();
        // Potentially unoptimized doing this every time.
        let payload: String = Message::payload_bytes_to_string(message.payload());
        // Size of the payload (STRING)
        let payload_len: usize = payload.len();

        let header_size: usize =
            length.len() + direction.len() + Part::get_standard_string_length();
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
            // What? (my future me is having trouble here, start + payload_slice_size is always
            // greater than payload_len, right...?)
            let stop = usize::min(start + payload_slice_size, payload_len); // Prevent out-of-bounds slicing
            let sliced_payload: &str = &payload[start..stop];
            put_payload_chars = stop; // Update position

            // Construct the full partition string
            part_buffer.clear();
            part_buffer.push_str(&direction);
            part_buffer.push_str(&part);
            part_buffer.push_str(sliced_payload);

            let length: String =
                part_buffer.len().to_string() + &Message::LENGTH_DELIMITER.to_string();

            // A whole message is [Lenght, Direction, Part, Payload]
            let part = Message::from_string(length + &part_buffer)?;
            parts.extend_from_slice(&part);
        }

        Ok(parts)
    }

    /// Merges all the `Message`s into a single `Message`.
    pub fn merge<T: AsRef<[Message]>>(parts: T) -> Result<Message, MessageError> {
        let parts: &[Message] = parts.as_ref();

        // Handle case where there are no parts
        if parts.is_empty() {
            return Err(MessageError::Partitioning("No parts to merge"));
        }

        // Extract direction from the first part
        let direction = parts[0].direction;

        let max_message_length: usize = parts.len() * DiscordBot::MAX_MESSAGE_LENGTH_ALLOWED;
        let mut payload_buffer: Vec<u8> = Vec::with_capacity(max_message_length);

        // Merge all parts
        for part in parts {
            payload_buffer.extend_from_slice(part.payload());
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
        let text = &text[..expected_len].trim();
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

/// Functions to aggregate and disaggregate `Messages`.
///
/// Simply put: takes lots of small `Message`s and return the biggest messages we can build, while
/// still being able to de-agregate those agregated messages into their smaller ones.
///
///
/// (length = the length of the message's total string representation(sent to discord))
///
/// In this context, aggregation is taking multiple "small" `Message`s and making fewer `Message`s
/// packed with multiple sub-`Message`.
///
/// For example if we have two `Message`s of total length 20 and let's say the header is of length
/// 10, the aggregated `Message`'s length will be 30
/// (10(header (fixed size)) + 10(payload of message 1) + 10(payload of message 2))
///
/// For simplicity, we manipulate strings.
pub struct Aggregator {}

impl Aggregator {
    /// At the end of the numerical length to mark that the lenght is finished. (Lazy-VarInt)
    const LENGTH_END_FLAG: &'static str = "*";

    /// Aggregates multiple messages into a single one.
    /// Conceptual example: [["12", "34", 56]] into [["123456"]].
    ///
    /// Note: Inputted messages will be partitionned if too large.
    pub fn aggregate<T: AsRef<[Message]>>(messages: T) -> Result<Vec<String>, MessageError> {
        let messages: &[Message] = messages.as_ref();

        // Partition messages that may need splitting.
        let parts: Vec<Message> = messages
            .iter()
            .map(|m| Partitioner::partition(m.clone(), DiscordBot::MAX_MESSAGE_LENGTH_ALLOWED))
            .collect::<Result<Vec<Vec<Message>>, MessageError>>()?
            .into_iter()
            .flatten()
            .collect();

        let mut aggregated: Vec<String> = Vec::new();
        let mut buffer = String::new();

        // Process each part to form a segment.
        for part in parts {
            let segment: &str = part.to_string();

            // If appending the segment would overflow the current buffer, flush it.
            if buffer.len() + segment.len() > DiscordBot::MAX_MESSAGE_LENGTH_ALLOWED {
                aggregated.push(buffer);
                buffer = String::new();
            }

            buffer.push_str(&segment);
        }

        // Append any remaining data.
        if !buffer.is_empty() {
            aggregated.push(buffer);
        }

        Ok(aggregated)
    }

    /// Disaggregates all aggregate parts from the current `AggregateMessage` object into multiple
    /// `Message`s.
    pub fn disaggregate(aggregate_message: &str) -> Result<Vec<Message>, MessageError> {
        let mut messages: Vec<Message> = Vec::new();
        let mut offset: usize = 0;
        let total_len: usize = aggregate_message.len();
        let mut messages_char_counter: usize = 0;

        // Suspicious convoluted loop; bugs may be hidden.
        while aggregate_message.len() != messages_char_counter {
            // Parse the length field until the '*' delimiter is found.
            let mut var_length = String::new();
            while offset < total_len {
                let c =
                    aggregate_message
                        .get(offset..offset + 1)
                        .ok_or(MessageError::Aggregation(
                            "Unexpected end of string while parsing length.",
                        ))?;
                offset += 1;
                if c == Self::LENGTH_END_FLAG {
                    if var_length.is_empty() {
                        return Err(MessageError::Aggregation(
                            "No digits found for variable length.",
                        ));
                    }
                    break;
                }
                var_length.push_str(c);
            }

            // Add length of the length.
            messages_char_counter += var_length.len() + Message::LENGTH_DELIMITER.len_utf8();
            let message_length: usize = var_length
                .parse()
                .map_err(|_| MessageError::Aggregation("Failed to parse the message length."))?;
            // And add the length of the header(except the Length) + payload.
            messages_char_counter += message_length;

            // Tries to read the first direction from the string.
            let direction = MessageDirection::from_string(&aggregate_message[offset..])?;
            offset += direction.to_string().len();

            let part = Part::from_string(&aggregate_message[offset..])?;
            offset += part.to_string().len();

            let payload: &str = aggregate_message
                .get(offset..message_length)
                .ok_or(MessageError::Aggregation("Failed to slice the payload."))?;

            // May be unoptimized, maybe use from_string().
            messages.push(Message::from_bytes(
                Message::payload_string_to_bytes(payload)?,
                direction,
            ));
        }

        Ok(messages)
    }
}
