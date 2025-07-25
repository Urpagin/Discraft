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

// TODO: Make this mess into smaller digestible functions.
impl Partitioner {
    /// Check if the length limit and the Message are compatible.
    /// (I.e., no, if the former is 0 or the latter's header size is less than the former.)
    fn check_is_partitionable(message: &Message, limit: usize) -> Result<String, MessageError> {
        println!("partition() input msg: {message:?}");
        println!("partition() input limit: {limit:?}");
        // Check for invalid `max` values
        if limit == 0 {
            return Err(MessageError::Partitioning(
                "partitioning divisor cannot be zero",
            ));
        }

        // fn is_partitionable()
        // fn check_is_partitionable()

        // ----- COMPUTE HEADER SIZE && CHECK
        // Potentially unoptimized doing this every time.
        let payload: String = Message::payload_bytes_to_string(message.payload());
        println!("payload: {payload:?}");
        // Size of the payload (STRING)
        let payload_len: usize = payload.len();
        println!("payload_len (string): {payload_len:?}");

        let header_size: usize = message.get_header_size();
        if limit <= header_size {
            return Err(MessageError::Partitioning(
                "length limit is too small to accommodate the header",
            ));
        }

        Ok(payload)
    }

    /// Computes the number of total parts the message will be split.
    /// Returns the number of total parts AND the size of the parts.
    fn compute_total_parts(limit: usize, header_size: usize, payload_len: usize) -> (usize, usize) {
        let payload_slice_size: usize = limit - header_size;
        // Compute the number of partitions we will need to create
        let whole_parts: usize = payload_len / payload_slice_size;
        let remainder: usize = payload_len % payload_slice_size;
        let total_parts = if remainder > 0 {
            whole_parts + 1
        } else {
            whole_parts
        };

        println!("There are {total_parts} parts");
        println!("The payload slice size {payload_slice_size}");
        println!("The remainder is {remainder}");

        (total_parts, payload_slice_size)
    }
    /// This function partitions BY TEXT, and not bytewise!
    /// Takes a message and returns smaller messages that all fit within the character limit.
    ///
    /// If the input message is already smaller than the max chars, it is returned.
    ///
    /// # ! IMPORTANT !
    ///
    /// IMPORTANT!!: Everything might just blow up if the message encoding is done with UTF-8 characters (non-ASCII).
    ///
    /// * The `limit` is a size in number of characters.
    pub fn partition(message: Message, limit: usize) -> Result<Vec<Message>, MessageError> {
        // Check: can the limit accommodate the message.
        let payload: String = Self::check_is_partitionable(&message, limit)?;

        // I call this function twice...
        let header_size: usize = message.get_header_size();
        let payload_len: usize = payload.len();

        println!("FLAG I");

        // The number of payload characters we can put while still being able to put the header.
        let (total_parts, payload_slice_size) =
            Self::compute_total_parts(limit, header_size, payload_len);

        // testing2 begin--

        // Where the partition will be stored each loop iteration
        let mut part_buffer: String = String::with_capacity(limit);

        // All the parts that make up the inputted message
        let mut parts: Vec<Message> = Vec::with_capacity(total_parts);

        let mut offset: usize = 0;
        let mut neg_offset: usize = 0;


        for i in 1..=total_parts {
            let part: String = Part::new(i, total_parts)?.to_string();
            println!("[FOR LOOP] payload.len()={}", payload.len());

            let start = (i - 1) * payload_slice_size;
            let end = if i != total_parts {
                (i * payload_slice_size) - neg_offset
            } else {
                payload.len()
            };
            let mut slice = payload[start..end].to_owned();

            // let mut slice: String = if i != total_parts {
            //     // Get whole parts
            //     payload[offset..((i - 1) * payload_slice_size)].to_owned()
            // } else {
            //     // Get the remainder
            //     payload[offset..].to_owned()
            // };

            //let mut slice = payload[range].to_owned();

            // if hex is len odd (badly cut). EXCEPT the last part.
            if slice.len() % 2 != 0 && i != total_parts {
                slice = payload[offset..(i * payload_slice_size - 1)].to_owned();
                // So that the next iteration will contain the removed hex nibble.
                //offset += payload_slice_size - 1;
                neg_offset += 1;
            }
            // if hex len is even (OK). EXCEPT the last part.
            if slice.len() % 2 == 0 && i != total_parts {
                // not total parts.
                // CHECK THIS: (original)
                //offset += payload_slice_size;
                offset += slice.len();
            }

            // if hex is odd ON THE LAST PART
            if slice.len() % 2 != 0 && i == total_parts {
                // Last part is not good:
                slice += "0";
                // Add 0 at the end to make it even.
            }

            // Construct the full partition string
            // That's dirty, no constructor?
            part_buffer.clear();
            part_buffer.push_str(&message.direction.to_string());
            part_buffer.push_str(&part);
            part_buffer.push_str(&slice);

            let length: String =
                part_buffer.len().to_string() + &Message::LENGTH_DELIMITER.to_string();

            // A whole message is [Length, Direction, Part, Payload]
            let part = Message::from_string(length + &part_buffer)?;
            parts.extend_from_slice(&part);
        }

        // testing2 end--
        return Ok(parts);

        println!("FLAG II");

        // The number of payload characters we have partitioned
        let mut put_payload_chars: usize = 0;
        // The current part number. Like 1/2 (current/total).
        let mut current_part: usize = 1;
        // All the parts that make up the inputted message
        let mut parts: Vec<Message> = Vec::with_capacity(total_parts);

        // Where the partition will be stored each loop iteration
        let mut part_buffer: String = String::with_capacity(limit);

        println!("FLAG III");

        // Exits when all the payload has been partitioned
        // Also, we have computed the number of parts, surely there's a way to not use a while
        // loop.

        // On the second and other round of the while loop,
        // this offset has to be added to the start idx in the slicing of the string payload.
        // Because we make sure the sliced string payload is always valid (no nibbles at the end).
        let mut hex_validity_offset: usize = 0;
        while put_payload_chars < payload.len() {
            println!("FLAG IV");
            let part: String = Part::new(current_part, total_parts)?.to_string();
            current_part += 1;

            let start: usize = put_payload_chars;
            // What? (my future me is having trouble here, start + payload_slice_size is always
            // greater than payload_len, right...?)
            let stop = usize::min(start + payload_slice_size, payload_len); // Prevent out-of-bounds slicing

            // --- dev in progress BEGIN ---

            // PROBLEM: THE SLICED PAYLOAD CANNOT CUT ANYWHERE, IT MUST CONTAIN A SEQUENCE OF BYTES
            // IN HEX, NO PARTIAL-BYTE NIBBLE THINGGY.

            // TODO: Ok, so we need to make a function that slices the payload string into parts,
            // the function needs to slice the hex in a valid manner, no nybbles.
            // However, it seems quite long and tedious to do with my tiny head, so I'm off...

            // Not even == partial hex.
            if &payload[start..stop].replace(" ", "").len() % 2 != 0 {
                hex_validity_offset += 1;
            }

            let sliced_payload: &str = &payload[start - hex_validity_offset..stop];

            // --- dev in progress END ---

            put_payload_chars = stop; // Update position
            println!("FLAG V");

            // Construct the full partition string
            part_buffer.clear();
            //part_buffer.push_str(&direction);
            part_buffer.push_str(&part);
            part_buffer.push_str(sliced_payload);

            let length: String =
                part_buffer.len().to_string() + &Message::LENGTH_DELIMITER.to_string();
            println!("FLAG VI");

            println!(
                "length.clone() + &part_buffer: {:?}",
                length.clone() + &part_buffer
            );

            // A whole message is [Lenght, Direction, Part, Payload]
            // TODO: IS THIS WHERE "FAILED TO DECODE HEX"?
            // TODO: IS THIS WHERE "FAILED TO DECODE HEX"?
            // TODO: IS THIS WHERE "FAILED TO DECODE HEX"?
            let part = Message::from_string(length + &part_buffer)?;
            parts.extend_from_slice(&part);
            println!("FLAG VII");
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

/// Represents what is the current part of a Message. 5 out of 10 for example.
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
            Err(MessageError::Partitioning(
                "total cannot exceed MAX_TOTAL. (too many parts, max is 255)",
            ))
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
        println!("tokens: {tokens:?}");
        // Parse current value
        let current_str = tokens.next().ok_or_else(|| {
            MessageError::Partitioning("Missing 'current' part in partitioning string")
        })?;
        let current: usize = usize::from_str_radix(current_str, 16)
            .map_err(|_| MessageError::Partitioning("Failed to parse 'current' as a hex number"))?;

        // Parse total value
        let total_str = tokens.next().ok_or_else(|| {
            MessageError::Partitioning("Missing 'total' part in partitioning string")
        })?;
        let total: usize = usize::from_str_radix(total_str, 16)
            .map_err(|_| MessageError::Partitioning("Failed to parse 'total' as a hex number"))?;

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

    /// Disaggregates all aggregate parts from the current `&str` into multiple
    /// `Message`s.
    pub fn disaggregate(aggregate_message: &str) -> Result<Vec<Message>, MessageError> {
        let mut messages: Vec<Message> = Vec::new();
        let mut offset: usize = 0;
        let total_len: usize = aggregate_message.len();
        let mut messages_char_counter: usize = 0;

        // Suspicious convoluted loop; bugs may be hidden.
        while messages_char_counter < aggregate_message.len() {
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
                if c == Message::LENGTH_DELIMITER.to_string() {
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
                .trim()
                .parse()
                .map_err(|_| MessageError::Aggregation("Failed to parse the message length."))?;

            // And add the length of the header(except the Length) + payload.
            messages_char_counter += message_length;

            // Tries to read the first direction from the string.
            let direction = MessageDirection::from_string(&aggregate_message[offset..])?;
            offset += direction.to_string().len();

            let part = Part::from_string(&aggregate_message[offset..])?;
            offset += part.to_string().len();

            // Length_len - (direction_len + part_len) = payload_len
            // Because Length_len does not contain itself.
            let payload_len: usize =
                message_length - (direction.to_string().len() + part.to_string().len());
            let payload: &str = aggregate_message
                .get(offset..offset + payload_len)
                .ok_or(MessageError::Aggregation("Failed to slice the payload."))?;
            offset += payload.len();

            // May be unoptimized, maybe use from_string().
            messages.push(Message::from_bytes(
                // TODO: STRING TO BYTES USED HERE !!!!!!!!!
                // TODO: STRING TO BYTES USED HERE !!!!!!!!!
                // TODO: STRING TO BYTES USED HERE !!!!!!!!!
                // TODO: STRING TO BYTES USED HERE !!!!!!!!!
                Message::payload_string_to_bytes(payload)?,
                direction,
            ));
        }

        Ok(messages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // For testing purposes, we define a dummy DiscordBot if one is not available.
    // Remove or adjust this module if your crate already defines DiscordBot.
    mod discord {
        pub struct DiscordBot;
        impl DiscordBot {
            // A small limit to force partitioning/aggregation in tests.
            pub const MAX_MESSAGE_LENGTH_ALLOWED: usize = 100;
        }
    }
    use crate::message::{Message, MessageDirection};
    use rand::{seq::IndexedRandom, Rng, RngCore};

    // Helper function to create a Message from a given payload string.
    fn create_message(payload: &str, direction: MessageDirection) -> Message {
        Message::from_bytes(payload.as_bytes(), direction)
    }

    #[test]
    fn test_partition_message_no_split() {
        // A short message should not be split.
        let payload = "Short message";
        let message = create_message(payload, MessageDirection::Clientbound);
        // Use a limit that is very generous compared to the message length.
        let limit = 1000;
        let parts = Partitioner::partition(message, limit).expect("Partitioning failed");
        assert_eq!(parts.len(), 1);

        // Verify that the payload (after decoding) equals the original.
        let encoded_payload = Message::payload_bytes_to_string(parts[0].payload());
        let decoded_bytes =
            Message::payload_string_to_bytes(&encoded_payload).expect("Decoding failed");
        assert_eq!(decoded_bytes, payload.as_bytes());
    }

    #[test]
    fn test_partition_message_split() {
        // let payload = "x".repeat(100);
        // let limit = 50;
        // let message = create_message(&payload, MessageDirection::Serverbound);
        // let parts = Partitioner::partition(message, limit).unwrap();
        // for part in parts {
        //     println!("part: {part:?}");
        // }
        // return;
        // A longer payload should be partitioned into multiple parts.
        //let payload = "This is a long message that should be split into multiple parts because it exceeds the allowed limit.";
        let payload = &"x".repeat(10000);
        let message = create_message(payload, MessageDirection::Serverbound);
        // Set a limit small enough to force splitting.
        let limit = 2000;
        let parts = Partitioner::partition(message, limit).expect("Partitioning failed");
        assert!(parts.len() > 1);

        // Reassemble the payload from the parts.
        let mut reconstructed = Vec::new();
        for part in parts {
            reconstructed.extend_from_slice(part.payload());
        }
        assert_eq!(reconstructed, payload.as_bytes());
    }

    #[test]
    fn test_partition_message_split2() {
        for _ in 0..100 {
            let byte_count: usize = rand::rng().random_range(2001..17_000);
            let mut data = Vec::with_capacity(byte_count);
            data.resize(byte_count, 0);
            rand::rng().fill_bytes(&mut data);
            // let rnd_hex: String = Message::payload_bytes_to_string(&data);

            let message = Message::from_bytes(data, MessageDirection::Serverbound);
            let messages = Partitioner::partition(message, 2000);
            assert!(
                messages.is_ok(),
                "Function returned an error: {:?}",
                messages
            );
        }
    }

    #[test]
    fn test_partition_invalid_limit_zero() {
        let payload = "Test payload";
        let message = create_message(payload, MessageDirection::Clientbound);
        let result = Partitioner::partition(message, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_partition_limit_too_small_for_header() {
        // The limit is set smaller than the minimum required header size.
        let payload = "Test";
        let message = create_message(payload, MessageDirection::Clientbound);
        let result = Partitioner::partition(message, 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_messages() {
        // Merge two messages and verify the payload concatenation.
        let payload1 = "Hello, ";
        let payload2 = "World!";
        let msg1 = create_message(payload1, MessageDirection::Clientbound);
        let msg2 = create_message(payload2, MessageDirection::Clientbound);
        let merged = Partitioner::merge(vec![msg1, msg2]).expect("Merge failed");

        // Decode the merged payload.
        let merged_encoded = Message::payload_bytes_to_string(merged.payload());
        let merged_bytes =
            Message::payload_string_to_bytes(&merged_encoded).expect("Decoding failed");
        let expected: Vec<u8> = [payload1.as_bytes(), payload2.as_bytes()].concat();
        assert_eq!(merged_bytes, expected);
    }

    #[test]
    fn test_empty_string_partition() {
        let input = "";
        let message = Message::from_string(input);

        assert!(message.is_ok(), "Not Ok()");
    }

    // #[test]
    // fn test_empty_string_merge() {
    //     return;
    //     todo!()
    //     let input = "";
    //     let message1 = Message::from_string(input).unwrap();
    //     let message2 = Message::from_string(input).unwrap();
    //
    //     let merged = Partitioner::merge(vec![message1, message2]);
    //     assert!(merged.is_ok(), "Not Ok()");
    // }

    // TODO: This does not pass the test because we do not build a message correctly.
    // Msg: [len, direction, part, payload]
    #[test]
    fn test_merge_messages2() {
        for _ in 0..300 {
            let mut messages = Vec::new();
            for _ in 0..100 {
                let byte_count: usize = rand::rng().random_range(1..324);
                let mut data = Vec::with_capacity(byte_count);
                data.resize(byte_count, 0);
                rand::rng().fill_bytes(&mut data);

                // Make a message with random payload.
                let random_msg = Message::from_bytes(&data, MessageDirection::Clientbound);

                let msg_hex = random_msg.to_string();

                let messages_vec =
                    Message::from_string(random_msg.to_string()).expect("Failed to merge messages");
                if let Some(msg) = messages_vec.first() {
                    // Get the first
                    messages.push(msg.clone());
                } else {
                    assert!(false, "Message is None. str: {msg_hex:?} / byte_count: {byte_count:?} / data: {data:?}");
                }
            }

            let messages_merged = Partitioner::merge(&messages).unwrap();
        }
    }

    #[test]
    fn test_merge_empty_parts() {
        let result = Partitioner::merge(Vec::<Message>::new());
        assert!(result.is_err());
    }

    #[test]
    fn test_aggregate_and_disaggregate() {
        // Create several messages.
        let payloads = vec!["Part one.", "Part two.", "Part three."];
        let messages: Vec<Message> = payloads
            .iter()
            .map(|p| create_message(p, MessageDirection::Serverbound))
            .collect();

        // Aggregate the messages.
        let aggregated_strings =
            Aggregator::aggregate(messages.clone()).expect("Aggregation failed");
        assert!(!aggregated_strings.is_empty());

        // Disaggregate each aggregated string.
        let mut disaggregated_messages = Vec::new();
        for agg in aggregated_strings {
            let parts = Aggregator::disaggregate(&agg).expect("Disaggregation failed");
            disaggregated_messages.extend(parts);
        }

        // Reconstruct the payload from the disaggregated messages.
        let mut reconstructed = Vec::new();
        for msg in disaggregated_messages {
            reconstructed.extend_from_slice(msg.payload());
        }
        let mut expected = Vec::new();
        for msg in messages {
            expected.extend_from_slice(msg.payload());
        }
        assert_eq!(reconstructed, expected);
    }

    #[test]
    fn test_disaggregate_invalid_string() {
        // An aggregate string that does not follow the proper format should error.
        let invalid_aggregate = "invalid message without proper length delimiter";
        let result = Aggregator::disaggregate(invalid_aggregate);
        assert!(result.is_err());
    }

    #[test]
    fn test_part_from_string_and_to_string() {
        // Verify that converting a Part to a string and back works correctly.
        let part = Part::new(1, 10).expect("Part creation failed");
        let part_str = part.to_string();
        let parsed_part = Part::from_string(&part_str).expect("Parsing Part from string failed");
        assert_eq!(part.current(), parsed_part.current());
        assert_eq!(part.total(), parsed_part.total());
    }

    #[test]
    fn test_part_from_string_invalid() {
        // Test several invalid partition strings.
        let invalid_strs = vec![
            "1/10",        // Not zero-padded and missing trailing space.
            "01/10/extra", // Extra token.
            "0110",        // Missing delimiter.
            "01/",         // Missing total.
            "/10",         // Missing current.
        ];
        for s in invalid_strs {
            assert!(Part::from_string(s).is_err());
        }
    }

    #[test]
    fn test_get_standard_string_length() {
        // The standard encoded Part (e.g., "01/01 ") should have a fixed length.
        let len = Part::get_standard_string_length();
        // Using the format "{:02X}/{:02X} " the expected length is 6.
        assert_eq!(len, 6);
    }
}
