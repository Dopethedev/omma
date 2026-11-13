//! Base64 without a dependency: the encoder for outbound data URIs and a strict
//! decoder for inline images pushed by an MCP client.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    Empty,
    InvalidCharacter(char),
    BadLength,
    MalformedDataUri,
    TooLarge { limit: u64 },
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Empty => write!(formatter, "base64 payload is empty"),
            DecodeError::InvalidCharacter(character) => {
                write!(formatter, "invalid base64 character {character:?}")
            }
            DecodeError::BadLength => write!(formatter, "base64 payload has an impossible length"),
            DecodeError::MalformedDataUri => {
                write!(formatter, "data URI is not base64-encoded")
            }
            DecodeError::TooLarge { limit } => write!(
                formatter,
                "base64 payload decodes to more than the {limit} byte limit"
            ),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Standard base64 with padding.
pub fn encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map(u32::from).unwrap_or(0);
        let b2 = chunk.get(2).copied().map(u32::from).unwrap_or(0);
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(triple >> 18) as usize & 0x3f] as char);
        out.push(ALPHABET[(triple >> 12) as usize & 0x3f] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(triple >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[triple as usize & 0x3f] as char
        } else {
            '='
        });
    }
    out
}

/// Longest encoded payload worth looking at for a given decoded ceiling: this
/// bounds the work before a single byte is decoded.
pub fn encoded_limit(max_decoded: u64) -> usize {
    (max_decoded / 3)
        .saturating_add(1)
        .saturating_mul(4)
        .try_into()
        .unwrap_or(usize::MAX)
}

/// Decode standard base64 (whitespace and a `data:...;base64,` prefix allowed,
/// URL-safe alphabet tolerated). Padding, length and size are all checked.
pub fn decode(input: &str, max_decoded: u64) -> Result<Vec<u8>, DecodeError> {
    let payload = strip_data_uri(input)?;
    if payload.len() > encoded_limit(max_decoded) {
        return Err(DecodeError::TooLarge { limit: max_decoded });
    }

    let mut out = Vec::with_capacity(payload.len() / 4 * 3 + 3);
    let mut accumulator: u32 = 0;
    let mut bits = 0u32;
    let mut padding = 0usize;
    let mut symbols = 0usize;

    for character in payload.chars() {
        if character.is_ascii_whitespace() {
            continue;
        }
        if character == '=' {
            padding += 1;
            if padding > 2 {
                return Err(DecodeError::InvalidCharacter('='));
            }
            continue;
        }
        if padding > 0 {
            // Data after padding, e.g. "QQ==QQ".
            return Err(DecodeError::InvalidCharacter(character));
        }
        let value = value_of(character).ok_or(DecodeError::InvalidCharacter(character))?;
        accumulator = (accumulator << 6) | u32::from(value);
        bits += 6;
        symbols += 1;
        if bits >= 8 {
            bits -= 8;
            out.push((accumulator >> bits) as u8);
            if out.len() as u64 > max_decoded {
                return Err(DecodeError::TooLarge { limit: max_decoded });
            }
        }
    }

    if symbols == 0 {
        return Err(DecodeError::Empty);
    }
    // A group of one symbol, or six dangling bits, cannot come from any byte string.
    if symbols % 4 == 1 || bits == 6 {
        return Err(DecodeError::BadLength);
    }
    if out.is_empty() {
        return Err(DecodeError::Empty);
    }
    Ok(out)
}

fn strip_data_uri(input: &str) -> Result<&str, DecodeError> {
    let trimmed = input.trim();
    let Some(rest) = trimmed.strip_prefix("data:") else {
        return Ok(trimmed);
    };
    let (meta, payload) = rest.split_once(',').ok_or(DecodeError::MalformedDataUri)?;
    if !meta.ends_with(";base64") {
        return Err(DecodeError::MalformedDataUri);
    }
    Ok(payload)
}

fn value_of(character: char) -> Option<u8> {
    match character {
        'A'..='Z' => Some(character as u8 - b'A'),
        'a'..='z' => Some(character as u8 - b'a' + 26),
        '0'..='9' => Some(character as u8 - b'0' + 52),
        '+' | '-' => Some(62),
        '/' | '_' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_length_class() {
        assert_eq!(encode(b""), "");
        for length in 1..64usize {
            let input: Vec<u8> = (0..length).map(|index| (index * 7 % 251) as u8).collect();
            let encoded = encode(&input);
            assert_eq!(
                decode(&encoded, 1024).expect("decode"),
                input,
                "length {length}"
            );
        }
    }

    #[test]
    fn accepts_a_data_uri_and_whitespace() {
        let encoded = encode(b"hello image");
        let uri = format!("data:image/png;base64,{}", encoded);
        assert_eq!(decode(&uri, 1024).expect("decode"), b"hello image");
        let wrapped = encoded
            .as_bytes()
            .chunks(8)
            .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(decode(&wrapped, 1024).expect("wrapped"), b"hello image");
    }

    #[test]
    fn refuses_malformed_input() {
        assert_eq!(decode("", 1024), Err(DecodeError::Empty));
        assert_eq!(
            decode("!!!!", 1024),
            Err(DecodeError::InvalidCharacter('!'))
        );
        assert_eq!(
            decode("QQ==QQ", 1024),
            Err(DecodeError::InvalidCharacter('Q'))
        );
        assert_eq!(decode("Q", 1024), Err(DecodeError::BadLength));
        assert_eq!(
            decode("data:image/png,raw", 1024),
            Err(DecodeError::MalformedDataUri)
        );
    }

    #[test]
    fn enforces_the_decoded_limit_before_allocating_much() {
        let big = vec![7u8; 4096];
        let encoded = encode(&big);
        assert_eq!(
            decode(&encoded, 1024),
            Err(DecodeError::TooLarge { limit: 1024 })
        );
        assert!(decode(&encoded, 4096).is_ok());
    }

    #[test]
    fn tolerates_the_url_safe_alphabet() {
        // "??>" encodes to "Pz8+" in standard and "Pz8-" in URL-safe base64.
        assert_eq!(decode("Pz8-", 16).expect("url safe"), b"??>");
    }
}
