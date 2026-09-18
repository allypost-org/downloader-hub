use std::{fmt, str};

#[allow(clippy::enum_glob_use)]
use self::Charset::*;

/// A MIME character set.
///
/// The string representation is normalized to upper case.
///
/// See <http://www.iana.org/assignments/character-sets/character-sets.xhtml>.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum Charset {
    Us_Ascii,
    Iso_8859_1,
    Iso_8859_2,
    Iso_8859_3,
    Iso_8859_4,
    Iso_8859_5,
    Iso_8859_6,
    Iso_8859_7,
    Iso_8859_8,
    Iso_8859_9,
    Iso_8859_10,
    Shift_Jis,
    Euc_Jp,
    Iso_2022_Kr,
    Euc_Kr,
    Iso_2022_Jp,
    Iso_2022_Jp_2,
    Iso_8859_6_E,
    Iso_8859_6_I,
    Iso_8859_8_E,
    Iso_8859_8_I,
    Gb2312,
    Big5,
    Koi8_R,
    Utf_8,
    /// An arbitrary charset specified as a string
    Ext(String),
}

impl Charset {
    fn label(&self) -> &str {
        match *self {
            Us_Ascii => "US-ASCII",
            Iso_8859_1 => "ISO-8859-1",
            Iso_8859_2 => "ISO-8859-2",
            Iso_8859_3 => "ISO-8859-3",
            Iso_8859_4 => "ISO-8859-4",
            Iso_8859_5 => "ISO-8859-5",
            Iso_8859_6 => "ISO-8859-6",
            Iso_8859_7 => "ISO-8859-7",
            Iso_8859_8 => "ISO-8859-8",
            Iso_8859_9 => "ISO-8859-9",
            Iso_8859_10 => "ISO-8859-10",
            Shift_Jis => "Shift-JIS",
            Euc_Jp => "EUC-JP",
            Iso_2022_Kr => "ISO-2022-KR",
            Euc_Kr => "EUC-KR",
            Iso_2022_Jp => "ISO-2022-JP",
            Iso_2022_Jp_2 => "ISO-2022-JP-2",
            Iso_8859_6_E => "ISO-8859-6-E",
            Iso_8859_6_I => "ISO-8859-6-I",
            Iso_8859_8_E => "ISO-8859-8-E",
            Iso_8859_8_I => "ISO-8859-8-I",
            Gb2312 => "GB2312",
            Big5 => "Big5",
            Koi8_R => "KOI8-R",
            Utf_8 => "UTF-8",
            Ext(ref s) => s,
        }
    }

    pub fn decode(&self, string: &[u8]) -> Option<String> {
        fn handle_encoding(
            encoding: &'static encoding_rs::Encoding,
            string: &[u8],
        ) -> Option<String> {
            encoding
                .decode_without_bom_handling_and_without_replacement(string)
                .map(|x| x.to_string())
        }

        match *self {
            Us_Ascii | Utf_8 => str::from_utf8(string)
                .map(std::string::ToString::to_string)
                .ok(),
            Iso_8859_1 => handle_encoding(encoding_rs::WINDOWS_1252, string),
            Iso_8859_2 => handle_encoding(encoding_rs::ISO_8859_2, string),
            Iso_8859_3 => handle_encoding(encoding_rs::ISO_8859_3, string),
            Iso_8859_4 => handle_encoding(encoding_rs::ISO_8859_4, string),
            Iso_8859_5 => handle_encoding(encoding_rs::ISO_8859_5, string),
            Iso_8859_6 | Iso_8859_6_E | Iso_8859_6_I => {
                handle_encoding(encoding_rs::ISO_8859_6, string)
            }
            Iso_8859_7 => handle_encoding(encoding_rs::ISO_8859_7, string),
            Iso_8859_8 | Iso_8859_8_E => handle_encoding(encoding_rs::ISO_8859_8, string),
            Iso_8859_9 => handle_encoding(encoding_rs::WINDOWS_1254, string),
            Iso_8859_10 => handle_encoding(encoding_rs::ISO_8859_10, string),
            Shift_Jis => handle_encoding(encoding_rs::SHIFT_JIS, string),
            Euc_Jp => handle_encoding(encoding_rs::EUC_JP, string),
            Euc_Kr => handle_encoding(encoding_rs::EUC_KR, string),
            Iso_2022_Jp => handle_encoding(encoding_rs::ISO_2022_JP, string),
            Iso_8859_8_I => handle_encoding(encoding_rs::ISO_8859_8_I, string),
            Big5 => handle_encoding(encoding_rs::BIG5, string),
            Koi8_R => handle_encoding(encoding_rs::KOI8_R, string),
            Iso_2022_Kr | Iso_2022_Jp_2 | Gb2312 | Ext(_) => None,
        }
    }
}

impl fmt::Display for Charset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

impl str::FromStr for Charset {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, anyhow::Error> {
        Ok(match s.to_ascii_uppercase().as_ref() {
            "US-ASCII" => Us_Ascii,
            "ISO-8859-1" => Iso_8859_1,
            "ISO-8859-2" => Iso_8859_2,
            "ISO-8859-3" => Iso_8859_3,
            "ISO-8859-4" => Iso_8859_4,
            "ISO-8859-5" => Iso_8859_5,
            "ISO-8859-6" => Iso_8859_6,
            "ISO-8859-7" => Iso_8859_7,
            "ISO-8859-8" => Iso_8859_8,
            "ISO-8859-9" => Iso_8859_9,
            "ISO-8859-10" => Iso_8859_10,
            "SHIFT-JIS" => Shift_Jis,
            "EUC-JP" => Euc_Jp,
            "ISO-2022-KR" => Iso_2022_Kr,
            "EUC-KR" => Euc_Kr,
            "ISO-2022-JP" => Iso_2022_Jp,
            "ISO-2022-JP-2" => Iso_2022_Jp_2,
            "ISO-8859-6-E" => Iso_8859_6_E,
            "ISO-8859-6-I" => Iso_8859_6_I,
            "ISO-8859-8-E" => Iso_8859_8_E,
            "ISO-8859-8-I" => Iso_8859_8_I,
            "GB2312" => Gb2312,
            "BIG5" => Big5,
            "KOI8-R" => Koi8_R,
            "UTF-8" => Utf_8,
            s => Ext(s.to_owned()),
        })
    }
}
