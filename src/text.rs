//! Turning the byte strings in a help file into Rust strings.
//!
//! WinHelp predates Unicode. Its text is in whatever Windows code page the author's machine
//! used, which may be a double-byte one. The `|SYSTEM` charset record names it only loosely:
//! Russian files say 0, and others hold values like 0x8080. The LCID record is what can be
//! trusted, and WinHelp 3.1 files have neither, so [`crate::HlpFile`] picks the encoding.

use encoding_rs::Encoding;

/// The 32 characters where Windows-1252 differs from Latin-1, covering bytes 0x80 to 0x9F.
const CP1252_HIGH: [char; 32] = [
	'\u{20AC}', '\u{FFFD}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{02C6}',
	'\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{FFFD}', '\u{017D}', '\u{FFFD}', '\u{FFFD}', '\u{2018}',
	'\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}', '\u{02DC}', '\u{2122}', '\u{0161}',
	'\u{203A}', '\u{0153}', '\u{FFFD}', '\u{017E}', '\u{0178}',
];

/// How the byte strings of one help file become text.
#[derive(Debug, Clone, Copy)]
pub enum Decoder {
	/// Windows-1252, keeping this crate's replacement character for its unassigned bytes.
	Windows1252,
	/// Each byte as the code point of the same number, so the bytes can be recovered.
	Bytes,
	/// Any other code page.
	Encoding(&'static Encoding),
}

impl Decoder {
	pub fn of(encoding: &'static Encoding) -> Self {
		if encoding == encoding_rs::WINDOWS_1252 { Self::Windows1252 } else { Self::Encoding(encoding) }
	}

	pub fn decode(self, bytes: &[u8]) -> String {
		match self {
			Self::Windows1252 => decode(bytes),
			Self::Bytes => bytes.iter().map(|&b| char::from(b)).collect(),
			Self::Encoding(encoding) => encoding.decode_without_bom_handling(bytes).0.into_owned(),
		}
	}

	pub fn decode_cstr(self, bytes: &[u8]) -> String {
		let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
		self.decode(&bytes[..end])
	}
}

/// The code page Windows uses for a language, or `None` for the Western ones, which are
/// Windows-1252 unless the text says otherwise.
pub fn encoding_for_lcid(lcid: u16) -> Option<&'static Encoding> {
	// Serbian, Bosnian, Azeri and Uzbek are each written in both Latin and Cyrillic, which
	// only the full LCID tells apart.
	let cyrillic_variant = matches!(lcid, 0x0C1A | 0x1C1A | 0x201A | 0x281A | 0x301A | 0x082C | 0x0843);
	let encoding = match lcid & 0x3FF {
		_ if cyrillic_variant => encoding_rs::WINDOWS_1251,
		0x04 if matches!(lcid, 0x0404 | 0x0C04 | 0x1404) => encoding_rs::BIG5,
		0x04 => encoding_rs::GBK,
		0x11 => encoding_rs::SHIFT_JIS,
		0x12 => encoding_rs::EUC_KR,
		0x1E => encoding_rs::WINDOWS_874,
		0x02 | 0x19 | 0x22 | 0x23 | 0x2F | 0x3F | 0x40 | 0x44 | 0x50 => encoding_rs::WINDOWS_1251,
		0x05 | 0x0E | 0x15 | 0x18 | 0x1A | 0x1B | 0x1C | 0x24 | 0x42 => encoding_rs::WINDOWS_1250,
		0x08 => encoding_rs::WINDOWS_1253,
		0x1F | 0x2C | 0x43 => encoding_rs::WINDOWS_1254,
		0x0D => encoding_rs::WINDOWS_1255,
		0x01 | 0x20 | 0x29 => encoding_rs::WINDOWS_1256,
		0x25..=0x27 => encoding_rs::WINDOWS_1257,
		0x2A => encoding_rs::WINDOWS_1258,
		_ => return None,
	};
	Some(encoding)
}

/// Decode `bytes` as Windows-1252.
pub fn decode(bytes: &[u8]) -> String {
	bytes
		.iter()
		.map(|&b| match b {
			0x80..=0x9f => CP1252_HIGH[usize::from(b) - 0x80],
			_ => char::from(b),
		})
		.collect()
}

/// Decode `bytes` as Windows-1252, stopping at the first NUL.
///
/// Some records store a string padded out to a fixed size, so the declared length is not
/// the string's length.
pub fn decode_cstr(bytes: &[u8]) -> String {
	let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
	decode(&bytes[..end])
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn ascii_passes_through() {
		assert_eq!(decode(b"Hello, world."), "Hello, world.");
	}

	// Latin-1 and Windows-1252 agree above 0x9F, so accented text needs no special case.
	#[test]
	fn high_latin1_bytes_map_to_their_own_code_points() {
		assert_eq!(decode(&[0xe9, 0xfc, 0xdf]), "éüß");
	}

	// The 0x80 to 0x9F range is the whole reason this is not just a Latin-1 cast: WinHelp
	// files are full of smart quotes and dashes that Latin-1 would turn into control codes.
	#[test]
	fn the_windows_range_maps_to_punctuation() {
		assert_eq!(decode(&[0x93, 0x94, 0x97, 0x85]), "\u{201C}\u{201D}\u{2014}\u{2026}");
		assert_eq!(decode(&[0x80]), "\u{20AC}");
	}

	#[test]
	fn unassigned_bytes_become_the_replacement_character() {
		assert_eq!(decode(&[0x81]), "\u{FFFD}");
	}

	#[test]
	fn a_double_byte_code_page_decodes_whole_characters() {
		let decoder = Decoder::of(encoding_for_lcid(0x0411).unwrap());
		assert_eq!(decoder.decode(&[0x83, 0x43, 0x83, 0x93, 0x83, 0x58]), "\u{30A4}\u{30F3}\u{30B9}");
	}

	#[test]
	fn a_lcid_picks_its_code_page_and_leaves_western_ones_out() {
		assert_eq!(encoding_for_lcid(0x0419), Some(encoding_rs::WINDOWS_1251));
		assert_eq!(encoding_for_lcid(0x0804), Some(encoding_rs::GBK));
		assert_eq!(encoding_for_lcid(0x0404), Some(encoding_rs::BIG5));
		assert_eq!(encoding_for_lcid(0x0409), None);
		assert_eq!(encoding_for_lcid(0x040C), None);
	}

	#[test]
	fn the_bytes_decoder_keeps_every_byte_recoverable() {
		let bytes = [0x41, 0x81, 0xFF];
		let decoded: Vec<u32> = Decoder::Bytes.decode(&bytes).chars().map(u32::from).collect();
		assert_eq!(decoded, [0x41, 0x81, 0xFF]);
	}

	#[test]
	fn a_c_string_stops_at_its_terminator() {
		assert_eq!(decode_cstr(b"Title\0padding\0"), "Title");
		assert_eq!(decode_cstr(b"No terminator"), "No terminator");
		assert_eq!(decode_cstr(b""), "");
	}
}
