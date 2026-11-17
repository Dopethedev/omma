//! _auto_redact
//!
//! Find things worth hiding before anything reads the image.
//!
//! Detection runs on a local OCR pass, using the words and their boxes. Nothing
//! here is clever: a short list of shapes a person would recognise as sensitive,
//! plus the Luhn check that tells a real card number from a random digit run.
//! Every match is reported back, so a caller can see what was hidden and where.

use crate::report::{Bbox, Line, Word};

/// One detected region, in the coordinates of the analyzed image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// What matched: `card`, `email` or `phone`.
    pub label: &'static str,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Padding around a word box, in pixels, so the glyph edges are covered too.
const PAD: u32 = 8;

/// Scan recognized words and lines, and return the boxes to paint out.
///
/// Words are matched first, because a phone number or an address is one word.
/// Lines are matched too: OCR splits `4111 1111 1111 1111` into four words, and
/// only the whole line has enough digits to recognise as a card number.
pub fn find(words: &[Word], lines: &[Line], image_width: u32, image_height: u32) -> Vec<Candidate> {
    let mut found = Vec::new();
    for word in words {
        let text = word.text.trim();
        if text.is_empty() {
            continue;
        }
        if let Some(label) = classify(text)
            && let Some(candidate) = clip(&word.bbox, label, image_width, image_height)
        {
            found.push(candidate);
        }
    }
    for line in lines {
        let text = line.text.trim();
        if text.is_empty() {
            continue;
        }
        if let Some(label) = classify_line(text)
            && let Some(candidate) = clip(&line.bbox, label, image_width, image_height)
        {
            // A line box covers its words, so the word boxes inside it would
            // only duplicate the same area.
            found
                .retain(|c| c.y + c.height <= candidate.y || c.y >= candidate.y + candidate.height);
            found.push(candidate);
        }
    }
    found
}

fn clip(bbox: &Bbox, label: &'static str, width: u32, height: u32) -> Option<Candidate> {
    let x = bbox.x.saturating_sub(PAD);
    let y = bbox.y.saturating_sub(PAD);
    let right = (bbox.x + bbox.width + PAD).min(width);
    let bottom = (bbox.y + bbox.height + PAD).min(height);
    if right <= x || bottom <= y {
        return None;
    }
    Some(Candidate {
        label,
        x,
        y,
        width: right - x,
        height: bottom - y,
    })
}

/// Match a whole line, where a card number may arrive with a label beside it.
fn classify_line(text: &str) -> Option<&'static str> {
    let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
    if (13..=19).contains(&digits.len()) && luhn(&digits) {
        return Some("card");
    }
    if text.split_whitespace().any(is_email) {
        return Some("email");
    }
    None
}

fn classify(text: &str) -> Option<&'static str> {
    let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
    let digit_ratio = digits.len() as f32 / text.chars().count().max(1) as f32;

    // A card number: 13 to 19 digits that pass Luhn. Checked first, because a
    // grouped card number also looks like a phone number.
    if (13..=19).contains(&digits.len()) && digit_ratio > 0.7 && luhn(&digits) {
        return Some("card");
    }
    if is_email(text) {
        return Some("email");
    }
    if is_phone(text, &digits, digit_ratio) {
        return Some("phone");
    }
    None
}

fn is_email(text: &str) -> bool {
    let mut parts = text.split('@');
    let (Some(local), Some(domain)) = (parts.next(), parts.next()) else {
        return false;
    };
    if parts.next().is_some() {
        return false;
    }
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && domain
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

fn is_phone(text: &str, digits: &str, digit_ratio: f32) -> bool {
    if digits.len() < 9 || digits.len() > 15 || digit_ratio < 0.6 {
        return false;
    }
    // Require a phone shape rather than any long number: a leading plus, or
    // separators between groups.
    let grouped = text.contains('-') || text.contains(' ') || text.contains('(');
    text.starts_with('+') || grouped
}

fn luhn(digits: &str) -> bool {
    let mut sum = 0u32;
    let mut double = false;
    for ch in digits.chars().rev() {
        let Some(d) = ch.to_digit(10) else {
            return false;
        };
        let mut value = d;
        if double {
            value *= 2;
            if value > 9 {
                value -= 9;
            }
        }
        sum += value;
        double = !double;
    }
    sum.is_multiple_of(10)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{Bbox, Norm};

    fn word(text: &str) -> Word {
        Word {
            text: text.to_string(),
            confidence: 90.0,
            bbox: Bbox {
                x: 10,
                y: 20,
                width: 100,
                height: 20,
                normalized: Norm {
                    x: 0.0,
                    y: 0.0,
                    width: 0.0,
                    height: 0.0,
                },
            },
            block: 0,
            paragraph: 0,
            line: 0,
        }
    }

    #[test]
    fn a_real_card_number_is_detected_and_a_random_run_is_not() {
        // 4111 1111 1111 1111 passes Luhn; 1234 5678 9012 3456 does not.
        assert_eq!(classify("4111 1111 1111 1111"), Some("card"));
        assert_eq!(classify("4111111111111111"), Some("card"));
        assert_eq!(classify("1234 5678 9012 3456"), None);
    }

    #[test]
    fn email_addresses_are_detected() {
        assert_eq!(classify("jane@example.com"), Some("email"));
        assert_eq!(classify("not-an-email@"), None);
        assert_eq!(classify("plainword"), None);
    }

    #[test]
    fn phone_numbers_need_a_phone_shape() {
        assert_eq!(classify("+44 20 7946 0958"), Some("phone"));
        assert_eq!(classify("555-0100-234"), Some("phone"));
        assert_eq!(classify("447188200193"), None);
    }

    #[test]
    fn boxes_are_padded_and_clamped_to_the_frame() {
        let found = find(&[word("jane@example.com")], &[], 1000, 1000);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].x, 2);
        assert_eq!(found[0].y, 12);
        assert_eq!(found[0].width, 116);
        assert_eq!(found[0].height, 36);

        // A box at the edge is clipped rather than running past it.
        let edge = find(&[word("jane@example.com")], &[], 40, 40);
        assert_eq!(edge[0].width, 40 - 2);
        assert_eq!(edge[0].height, 40 - 12);
    }
}
