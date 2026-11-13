//! ir-codec - byte encodings shown and accepted by the app.
//!
//! Exists so captured and typed bytes can be shown and entered as hex,
//! binary, decimal or Base64 with one set of tested rules. Parsers are
//! lenient about separators and prefixes and report the character offset of
//! the first problem so the UI can point at it. Offsets count Unicode scalar
//! values (chars) in the input.

use std::fmt;

use base64::engine::general_purpose::STANDARD;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use base64::{DecodeError, Engine as _, alphabet};

/// A byte encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Encoding {
    Hex,
    Binary,
    Decimal,
    Base64,
}

impl Encoding {
    pub const ALL: [Encoding; 4] = [Encoding::Hex, Encoding::Binary, Encoding::Decimal, Encoding::Base64];
}

/// Layout of formatted output. Base64 output is never wrapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatOptions {
    /// Uppercase hex digits.
    pub uppercase: bool,
    /// Bytes per line for hex, binary and decimal; 0 means one line.
    pub bytes_per_line: usize,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self { uppercase: true, bytes_per_line: 16 }
    }
}

/// What is wrong with the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    /// A character that is not allowed in this encoding.
    InvalidChar(char),
    /// A hex token with an odd number of digits.
    OddHexDigits,
    /// Binary digits that do not add up to whole bytes.
    IncompleteByte,
    /// A decimal value above 255.
    ValueTooLarge(u64),
    /// Base64 that does not decode (wrong length or padding).
    InvalidBase64,
}

/// A parse failure at a character offset of the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub kind: ErrorKind,
    pub offset: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ErrorKind::InvalidChar(c) => write!(f, "unexpected character {c:?} at position {}", self.offset),
            ErrorKind::OddHexDigits => {
                write!(f, "hex group at position {} has an odd number of digits", self.offset)
            }
            ErrorKind::IncompleteByte => {
                write!(f, "binary digits at position {} do not make whole bytes", self.offset)
            }
            ErrorKind::ValueTooLarge(v) => write!(f, "value {v} at position {} is above 255", self.offset),
            ErrorKind::InvalidBase64 => write!(f, "invalid Base64 near position {}", self.offset),
        }
    }
}

impl std::error::Error for ParseError {}

/// Formats bytes in the given encoding.
pub fn format(bytes: &[u8], encoding: Encoding, options: &FormatOptions) -> String {
    let one: fn(&u8) -> String = match encoding {
        Encoding::Base64 => return STANDARD.encode(bytes),
        Encoding::Hex if options.uppercase => |b| format!("{b:02X}"),
        Encoding::Hex => |b| format!("{b:02x}"),
        Encoding::Binary => |b| format!("{b:08b}"),
        Encoding::Decimal => |b| b.to_string(),
    };
    let per_line = if options.bytes_per_line == 0 { bytes.len().max(1) } else { options.bytes_per_line };
    bytes
        .chunks(per_line)
        .map(|line| line.iter().map(one).collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parses text in the given encoding. Empty or separator-only input gives
/// no bytes.
pub fn parse(text: &str, encoding: Encoding) -> Result<Vec<u8>, ParseError> {
    match encoding {
        Encoding::Hex => parse_hex(text),
        Encoding::Binary => parse_binary(text),
        Encoding::Decimal => parse_decimal(text),
        Encoding::Base64 => parse_base64(text),
    }
}

fn error(kind: ErrorKind, offset: usize) -> ParseError {
    ParseError { kind, offset }
}

/// A run of non-separator characters: its chars with their offsets.
type Token = Vec<(usize, char)>;

/// Splits text into tokens at characters for which `is_sep` holds.
fn tokens(text: &str, is_sep: impl Fn(char) -> bool) -> Vec<Token> {
    let mut out = Vec::new();
    let mut current = Token::new();
    for (offset, c) in text.chars().enumerate() {
        if is_sep(c) {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
        } else {
            current.push((offset, c));
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Removes a two-character prefix such as `0x` (case-insensitive marker).
fn strip_prefix(token: &[(usize, char)], marker: char) -> &[(usize, char)] {
    match token {
        [(_, '0'), (_, m), rest @ ..] if m.eq_ignore_ascii_case(&marker) => rest,
        _ => token,
    }
}

fn is_byte_sep(c: char) -> bool {
    c.is_whitespace() || matches!(c, ',' | ':' | ';' | '-')
}

fn parse_hex(text: &str) -> Result<Vec<u8>, ParseError> {
    let mut out = Vec::new();
    for token in tokens(text, is_byte_sep) {
        let start = token[0].0;
        let digits = strip_prefix(&token, 'x');
        if digits.is_empty() {
            return Err(error(ErrorKind::InvalidChar(token[1].1), token[1].0));
        }
        let mut values = Vec::with_capacity(digits.len());
        for &(offset, c) in digits {
            let v = c.to_digit(16).ok_or(error(ErrorKind::InvalidChar(c), offset))?;
            values.push(v as u8);
        }
        if values.len() % 2 != 0 {
            return Err(error(ErrorKind::OddHexDigits, start));
        }
        out.extend(values.chunks(2).map(|pair| pair[0] << 4 | pair[1]));
    }
    Ok(out)
}

fn parse_binary(text: &str) -> Result<Vec<u8>, ParseError> {
    let mut bits = Vec::new();
    for token in tokens(text, is_byte_sep) {
        for &(offset, c) in strip_prefix(&token, 'b') {
            match c {
                '0' => bits.push((offset, 0u8)),
                '1' => bits.push((offset, 1u8)),
                _ => return Err(error(ErrorKind::InvalidChar(c), offset)),
            }
        }
    }
    if bits.len() % 8 != 0 {
        let first_of_last = bits[bits.len() / 8 * 8].0;
        return Err(error(ErrorKind::IncompleteByte, first_of_last));
    }
    Ok(bits.chunks(8).map(|byte| byte.iter().fold(0u8, |acc, &(_, bit)| acc << 1 | bit)).collect())
}

fn parse_decimal(text: &str) -> Result<Vec<u8>, ParseError> {
    let sep = |c: char| c.is_whitespace() || matches!(c, ',' | ';');
    let mut out = Vec::new();
    for token in tokens(text, sep) {
        let start = token[0].0;
        let mut value: u64 = 0;
        for &(offset, c) in &token {
            let digit = c.to_digit(10).ok_or(error(ErrorKind::InvalidChar(c), offset))?;
            value = value.saturating_mul(10).saturating_add(u64::from(digit));
        }
        let byte = u8::try_from(value).map_err(|_| error(ErrorKind::ValueTooLarge(value), start))?;
        out.push(byte);
    }
    Ok(out)
}

fn parse_base64(text: &str) -> Result<Vec<u8>, ParseError> {
    let chars: Vec<(usize, char)> = text.chars().enumerate().filter(|(_, c)| !c.is_whitespace()).collect();
    let url_safe = chars.iter().find_map(|&(_, c)| match c {
        '-' | '_' => Some(true),
        '+' | '/' => Some(false),
        _ => None,
    });
    let url_safe = url_safe.unwrap_or(false);
    for &(offset, c) in &chars {
        let ok = c.is_ascii_alphanumeric()
            || c == '='
            || if url_safe { matches!(c, '-' | '_') } else { matches!(c, '+' | '/') };
        if !ok {
            return Err(error(ErrorKind::InvalidChar(c), offset));
        }
    }
    let compact: String = chars.iter().map(|&(_, c)| c).collect();
    let engine = if url_safe { &URL_SAFE_LENIENT } else { &STANDARD_LENIENT };
    let at = |i: usize| chars.get(i).map_or(text.chars().count(), |&(offset, _)| offset);
    engine.decode(compact.as_bytes()).map_err(|e| match e {
        DecodeError::InvalidByte(i, b) => error(ErrorKind::InvalidChar(char::from(b)), at(i)),
        DecodeError::InvalidLastSymbol { offset, .. } => error(ErrorKind::InvalidBase64, at(offset)),
        _ => error(ErrorKind::InvalidBase64, text.chars().count()),
    })
}

const LENIENT: GeneralPurposeConfig =
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent);
const STANDARD_LENIENT: GeneralPurpose = GeneralPurpose::new(&alphabet::STANDARD, LENIENT);
const URL_SAFE_LENIENT: GeneralPurpose = GeneralPurpose::new(&alphabet::URL_SAFE, LENIENT);

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> FormatOptions {
        FormatOptions::default()
    }

    fn err(kind: ErrorKind, offset: usize) -> Result<Vec<u8>, ParseError> {
        Err(ParseError { kind, offset })
    }

    // ---- format ----

    #[test]
    fn plan_example_in_all_encodings() {
        let b = [0xFE, 0x44, 0x00];
        assert_eq!(format(&b, Encoding::Hex, &opts()), "FE 44 00");
        assert_eq!(format(&b, Encoding::Binary, &opts()), "11111110 01000100 00000000");
        assert_eq!(format(&b, Encoding::Decimal, &opts()), "254 68 0");
        assert_eq!(format(&b, Encoding::Base64, &opts()), "/kQA");
    }

    #[test]
    fn lowercase_hex_option() {
        let o = FormatOptions { uppercase: false, ..opts() };
        assert_eq!(format(&[0xAB], Encoding::Hex, &o), "ab");
    }

    #[test]
    fn lines_wrap_after_bytes_per_line() {
        let o = FormatOptions { bytes_per_line: 2, ..opts() };
        assert_eq!(format(&[1, 2, 3, 4, 5], Encoding::Hex, &o), "01 02\n03 04\n05");
        assert_eq!(format(&[1, 2, 3], Encoding::Decimal, &o), "1 2\n3");
    }

    #[test]
    fn zero_bytes_per_line_is_one_line() {
        let o = FormatOptions { bytes_per_line: 0, ..opts() };
        assert_eq!(format(&[0; 20], Encoding::Decimal, &o).lines().count(), 1);
    }

    #[test]
    fn base64_is_never_wrapped() {
        let o = FormatOptions { bytes_per_line: 2, ..opts() };
        assert_eq!(format(&[1, 2, 3, 4, 5, 6], Encoding::Base64, &o), "AQIDBAUG");
    }

    #[test]
    fn empty_input_formats_empty() {
        for e in Encoding::ALL {
            assert_eq!(format(&[], e, &opts()), "");
        }
    }

    // ---- round trip ----

    #[test]
    fn all_byte_values_round_trip_in_every_encoding() {
        let all: Vec<u8> = (0..=255).collect();
        for e in Encoding::ALL {
            let text = format(&all, e, &opts());
            assert_eq!(parse(&text, e), Ok(all.clone()), "{e:?}");
        }
    }

    // ---- hex ----

    #[test]
    fn hex_accepts_separators_prefixes_and_case() {
        assert_eq!(parse("fe:44,00", Encoding::Hex), Ok(vec![0xFE, 0x44, 0x00]));
        assert_eq!(parse("0xFE 0X44\n0x00", Encoding::Hex), Ok(vec![0xFE, 0x44, 0x00]));
        assert_eq!(parse("FE4400", Encoding::Hex), Ok(vec![0xFE, 0x44, 0x00]));
        assert_eq!(parse("fe-44;00\t", Encoding::Hex), Ok(vec![0xFE, 0x44, 0x00]));
    }

    #[test]
    fn hex_odd_group_is_error_at_group_start() {
        assert_eq!(parse("FE 4 00", Encoding::Hex), err(ErrorKind::OddHexDigits, 3));
        assert_eq!(parse("0xF 0x4", Encoding::Hex), err(ErrorKind::OddHexDigits, 0));
    }

    #[test]
    fn hex_invalid_char_reports_offset() {
        assert_eq!(parse("FE 4G", Encoding::Hex), err(ErrorKind::InvalidChar('G'), 4));
    }

    #[test]
    fn hex_empty_and_separator_only_is_empty() {
        assert_eq!(parse("", Encoding::Hex), Ok(vec![]));
        assert_eq!(parse(" , \n", Encoding::Hex), Ok(vec![]));
    }

    // ---- binary ----

    #[test]
    fn binary_accepts_prefix_and_nibble_groups() {
        assert_eq!(parse("0b11111110 0100 0100", Encoding::Binary), Ok(vec![0xFE, 0x44]));
        assert_eq!(parse("00000001,00000010", Encoding::Binary), Ok(vec![1, 2]));
    }

    #[test]
    fn binary_incomplete_byte_reports_start_of_last_byte() {
        // 9 bits: the second byte starts at the 9th digit (offset 9 after a space).
        assert_eq!(parse("11111111 1", Encoding::Binary), err(ErrorKind::IncompleteByte, 9));
    }

    #[test]
    fn binary_invalid_char() {
        assert_eq!(parse("0102", Encoding::Binary), err(ErrorKind::InvalidChar('2'), 3));
    }

    // ---- decimal ----

    #[test]
    fn decimal_accepts_common_separators() {
        assert_eq!(parse("254, 68\n0;1", Encoding::Decimal), Ok(vec![254, 68, 0, 1]));
    }

    #[test]
    fn decimal_rejects_values_above_255() {
        assert_eq!(parse("1 256", Encoding::Decimal), err(ErrorKind::ValueTooLarge(256), 2));
        assert_eq!(
            parse("99999999999999999999999", Encoding::Decimal),
            err(ErrorKind::ValueTooLarge(u64::MAX), 0)
        );
    }

    #[test]
    fn decimal_invalid_char() {
        assert_eq!(parse("12 -3", Encoding::Decimal), err(ErrorKind::InvalidChar('-'), 3));
        assert_eq!(parse("0x10", Encoding::Decimal), err(ErrorKind::InvalidChar('x'), 1));
    }

    // ---- base64 ----

    #[test]
    fn base64_standard_urlsafe_padded_unpadded() {
        assert_eq!(parse("+uwgVQA=", Encoding::Base64), Ok(vec![0xFA, 0xEC, 0x20, 0x55, 0x00]));
        assert_eq!(parse("-uwgVQA=", Encoding::Base64), Ok(vec![0xFA, 0xEC, 0x20, 0x55, 0x00]));
        assert_eq!(parse("-uwgVQA", Encoding::Base64), Ok(vec![0xFA, 0xEC, 0x20, 0x55, 0x00]));
        assert_eq!(parse("Zg==", Encoding::Base64), Ok(vec![b'f']));
        assert_eq!(parse("Zg", Encoding::Base64), Ok(vec![b'f']));
    }

    #[test]
    fn base64_ignores_whitespace() {
        assert_eq!(parse(" Zm9v\n YmFy ", Encoding::Base64), Ok(b"foobar".to_vec()));
    }

    #[test]
    fn base64_invalid_char_reports_original_offset() {
        assert_eq!(parse("Zm 9!", Encoding::Base64), err(ErrorKind::InvalidChar('!'), 4));
    }

    #[test]
    fn base64_mixed_alphabets_are_rejected() {
        let r = parse("+-AA", Encoding::Base64);
        assert!(matches!(r, Err(ParseError { kind: ErrorKind::InvalidChar(_), .. })), "{r:?}");
    }

    #[test]
    fn base64_bad_length_is_invalid() {
        assert_eq!(parse("Z", Encoding::Base64), err(ErrorKind::InvalidBase64, 1));
    }
}
