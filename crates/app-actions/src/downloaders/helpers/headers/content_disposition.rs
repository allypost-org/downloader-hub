//! The `Content-Disposition` header and associated types.
//!
//! # References
//! - "The Content-Disposition Header Field":
//!   <https://datatracker.ietf.org/doc/html/rfc2183>
//! - "The Content-Disposition Header Field in the Hypertext Transfer Protocol (HTTP)":
//!   <https://datatracker.ietf.org/doc/html/rfc6266>
//! - "Returning Values from Forms: multipart/form-data":
//!   <https://datatracker.ietf.org/doc/html/rfc7578>
//! - Browser conformance tests at: <http://greenbytes.de/tech/tc2231/>
//! - IANA assignment: <http://www.iana.org/assignments/cont-disp/cont-disp.xhtml>

use std::{fmt, str::FromStr, sync::LazyLock};

use http::header;
use language_tags::LanguageTag;
use percent_encoding::{AsciiSet, CONTROLS};
use regex::Regex;

use super::common::charset::Charset;

const HTTP_VALUE: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'%')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'-')
    .add(b'>')
    .add(b'?')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'{')
    .add(b'}');

/// The value part of an extended parameter consisting of three parts:
/// - The REQUIRED character set name (`charset`).
/// - The OPTIONAL language information (`language_tag`).
/// - A character sequence representing the actual value (`value`), separated by single quotes.
///
/// It is defined in [RFC 5987 §3.2](https://datatracker.ietf.org/doc/html/rfc5987#section-3.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtendedValue {
    /// The character set that is used to encode the `value` to a string.
    pub charset: Charset,

    /// The human language details of the `value`, if available.
    pub language_tag: Option<LanguageTag>,

    /// The parameter value, as expressed in octets.
    pub value: Vec<u8>,
}

impl ExtendedValue {
    #[allow(dead_code)]
    #[must_use]
    pub fn try_decode(&self) -> Option<String> {
        self.charset.decode(&self.value)
    }
}

impl fmt::Display for ExtendedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let encoded_value = percent_encoding::percent_encode(&self.value[..], HTTP_VALUE);
        if let Some(ref lang) = self.language_tag {
            write!(f, "{}'{}'{}", self.charset, lang, encoded_value)
        } else {
            write!(f, "{}''{}", self.charset, encoded_value)
        }
    }
}

/// Split at the index of the first `needle` if it exists or at the end.
#[inline]
fn split_once(haystack: &str, needle: char) -> (&str, &str) {
    haystack.find(needle).map_or_else(
        || (haystack, ""),
        |sc| {
            let (first, last) = haystack.split_at(sc);
            (first, last.split_at(1).1)
        },
    )
}

/// Split at the index of the first `needle` if it exists or at the end, trim the right of the
/// first part and the left of the last part.
#[inline]
fn split_once_and_trim(haystack: &str, needle: char) -> (&str, &str) {
    let (first, last) = split_once(haystack, needle);
    (first.trim_end(), last.trim_start())
}

/// The implied disposition of the content of the HTTP body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispositionType {
    /// Inline implies default processing.
    Inline,

    /// Attachment implies that the recipient should prompt the user to save the response locally,
    /// rather than process it normally (as per its media type).
    Attachment,

    /// Used in *multipart/form-data* as defined in
    /// [RFC 7578](https://datatracker.ietf.org/doc/html/rfc7578) to carry the field name and
    /// optional filename.
    FormData,

    /// Extension type. Should be handled by recipients the same way as Attachment.
    Ext(String),
}

impl<'a> From<&'a str> for DispositionType {
    fn from(origin: &'a str) -> Self {
        if origin.eq_ignore_ascii_case("inline") {
            Self::Inline
        } else if origin.eq_ignore_ascii_case("attachment") {
            Self::Attachment
        } else if origin.eq_ignore_ascii_case("form-data") {
            Self::FormData
        } else {
            Self::Ext(origin.to_owned())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum DispositionParam {
    /// For [`DispositionType::FormData`] (i.e. *multipart/form-data*), the name of an field from
    /// the form.
    Name(String),

    /// A plain file name.
    ///
    /// It is [not supposed](https://datatracker.ietf.org/doc/html/rfc6266#appendix-D) to contain
    /// any non-ASCII characters when used in a *Content-Disposition* HTTP response header, where
    /// [`FilenameExt`](DispositionParam::FilenameExt) with charset UTF-8 may be used instead
    /// in case there are Unicode characters in file names.
    Filename(String),

    /// An extended file name. It must not exist for `ContentType::Formdata` according to
    /// [RFC 7578 §4.2](https://datatracker.ietf.org/doc/html/rfc7578#section-4.2).
    FilenameExt(ExtendedValue),

    /// An unrecognized regular parameter as defined in
    /// [RFC 5987 §3.2.1](https://datatracker.ietf.org/doc/html/rfc5987#section-3.2.1) as
    /// `reg-parameter`, in
    /// [RFC 6266 §4.1](https://datatracker.ietf.org/doc/html/rfc6266#section-4.1) as
    /// `token "=" value`. Recipients should ignore unrecognizable parameters.
    Unknown(String, String),

    /// An unrecognized extended parameter as defined in
    /// [RFC 5987 §3.2.1](https://datatracker.ietf.org/doc/html/rfc5987#section-3.2.1) as
    /// `ext-parameter`, in
    /// [RFC 6266 §4.1](https://datatracker.ietf.org/doc/html/rfc6266#section-4.1) as
    /// `ext-token "=" ext-value`. The single trailing asterisk is not included. Recipients should
    /// ignore unrecognizable parameters.
    UnknownExt(String, ExtendedValue),
}

#[allow(dead_code)]
impl DispositionParam {
    /// Returns `true` if the parameter is [`Name`](DispositionParam::Name).
    #[inline]
    #[must_use]
    pub const fn is_name(&self) -> bool {
        self.as_name().is_some()
    }

    /// Returns `true` if the parameter is [`Filename`](DispositionParam::Filename).
    #[inline]
    #[must_use]
    pub const fn is_filename(&self) -> bool {
        self.as_filename().is_some()
    }

    /// Returns `true` if the parameter is [`FilenameExt`](DispositionParam::FilenameExt).
    #[inline]
    #[must_use]
    pub const fn is_filename_ext(&self) -> bool {
        self.as_filename_ext().is_some()
    }

    /// Returns `true` if the parameter is [`Unknown`](DispositionParam::Unknown) and the `name`
    #[inline]
    /// matches.
    pub fn is_unknown<T: AsRef<str>>(&self, name: T) -> bool {
        self.as_unknown(name).is_some()
    }

    /// Returns `true` if the parameter is [`UnknownExt`](DispositionParam::UnknownExt) and the
    /// `name` matches.
    #[inline]
    pub fn is_unknown_ext<T: AsRef<str>>(&self, name: T) -> bool {
        self.as_unknown_ext(name).is_some()
    }

    /// Returns the name if applicable.
    #[inline]
    #[must_use]
    pub const fn as_name(&self) -> Option<&str> {
        match self {
            Self::Name(name) => Some(name.as_str()),
            _ => None,
        }
    }

    /// Returns the filename if applicable.
    #[inline]
    #[must_use]
    pub const fn as_filename(&self) -> Option<&str> {
        match self {
            Self::Filename(filename) => Some(filename.as_str()),
            _ => None,
        }
    }

    /// Returns the filename* if applicable.
    #[inline]
    #[must_use]
    pub const fn as_filename_ext(&self) -> Option<&ExtendedValue> {
        match self {
            Self::FilenameExt(value) => Some(value),
            _ => None,
        }
    }

    /// Returns the value of the unrecognized regular parameter if it is
    /// [`Unknown`](DispositionParam::Unknown) and the `name` matches.
    #[inline]
    pub fn as_unknown<T: AsRef<str>>(&self, name: T) -> Option<&str> {
        match self {
            Self::Unknown(ext_name, value) if ext_name.eq_ignore_ascii_case(name.as_ref()) => {
                Some(value.as_str())
            }
            _ => None,
        }
    }

    /// Returns the value of the unrecognized extended parameter if it is
    /// [`Unknown`](DispositionParam::Unknown) and the `name` matches.
    #[inline]
    pub fn as_unknown_ext<T: AsRef<str>>(&self, name: T) -> Option<&ExtendedValue> {
        match self {
            Self::UnknownExt(ext_name, value) if ext_name.eq_ignore_ascii_case(name.as_ref()) => {
                Some(value)
            }
            _ => None,
        }
    }
}

pub fn parse_extended_value(val: &str) -> anyhow::Result<ExtendedValue> {
    // Break into three pieces separated by the single-quote character
    let mut parts = val.splitn(3, '\'');

    // Interpret the first piece as a Charset
    let charset: Charset = match parts.next() {
        None => anyhow::bail!("invalid charset"),
        Some(n) => FromStr::from_str(n)?,
    };

    // Interpret the second piece as a language tag
    let language_tag: Option<LanguageTag> = match parts.next() {
        None => anyhow::bail!("invalid language tag"),
        Some("") => None,
        Some(s) => match s.parse() {
            Ok(lt) => Some(lt),
            Err(_) => anyhow::bail!("invalid language tag"),
        },
    };

    // Interpret the third piece as a sequence of value characters
    let value: Vec<u8> = match parts.next() {
        None => anyhow::bail!("invalid value"),
        Some(v) => percent_encoding::percent_decode(v.as_bytes()).collect(),
    };

    Ok(ExtendedValue {
        charset,
        language_tag,
        value,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentDisposition {
    /// The disposition type
    pub disposition: DispositionType,

    /// Disposition parameters
    pub parameters: Vec<DispositionParam>,
}

#[allow(dead_code)]
impl ContentDisposition {
    /// Constructs a Content-Disposition header suitable for downloads.
    ///
    /// # Examples
    /// ```
    /// use actix_web::http::header::{ContentDisposition, TryIntoHeaderValue as _};
    ///
    /// let cd = ContentDisposition::attachment("files.zip");
    ///
    /// let cd_val = cd.try_into_value().unwrap();
    /// assert_eq!(cd_val, "attachment; filename=\"files.zip\"");
    /// ```
    pub fn attachment(filename: impl Into<String>) -> Self {
        Self {
            disposition: DispositionType::Attachment,
            parameters: vec![DispositionParam::Filename(filename.into())],
        }
    }

    /// Parse a raw Content-Disposition header value.
    pub fn from_raw(hv: &header::HeaderValue) -> anyhow::Result<Self> {
        // `header::from_one_raw_str` invokes `hv.to_str` which assumes `hv` contains only visible
        //  ASCII characters. So `hv.as_bytes` is necessary here.
        let hv = String::from_utf8(hv.as_bytes().to_vec())?;

        let (disp_type, mut left) = split_once_and_trim(hv.as_str().trim(), ';');
        if disp_type.is_empty() {
            anyhow::bail!("empty disposition type");
        }

        let mut cd = Self {
            disposition: disp_type.into(),
            parameters: Vec::new(),
        };

        while !left.is_empty() {
            let (param_name, new_left) = split_once_and_trim(left, '=');
            if param_name.is_empty() || param_name == "*" || new_left.is_empty() {
                anyhow::bail!("invalid parameter name");
            }
            left = new_left;
            if let Some(param_name) = param_name.strip_suffix('*') {
                // extended parameters
                let (ext_value, new_left) = split_once_and_trim(left, ';');
                left = new_left;
                let ext_value = parse_extended_value(ext_value)?;

                let param = if param_name.eq_ignore_ascii_case("filename") {
                    DispositionParam::FilenameExt(ext_value)
                } else {
                    DispositionParam::UnknownExt(param_name.to_owned(), ext_value)
                };
                cd.parameters.push(param);
            } else {
                // regular parameters
                let value = if left.starts_with('\"') {
                    // quoted-string: defined in RFC 6266 -> RFC 2616 Section 3.6
                    let mut escaping = false;
                    let mut quoted_string = vec![];
                    let mut end = None;
                    // search for closing quote
                    for (i, &c) in left.as_bytes().iter().skip(1).enumerate() {
                        if escaping {
                            escaping = false;
                            quoted_string.push(c);
                        } else if c == 0x5c {
                            // backslash
                            escaping = true;
                        } else if c == 0x22 {
                            // double quote
                            end = Some(i + 1); // cuz skipped 1 for the leading quote
                            break;
                        } else {
                            quoted_string.push(c);
                        }
                    }
                    left = &left[end.ok_or_else(|| anyhow::anyhow!("no closing quote"))? + 1..];
                    left = split_once(left, ';').1.trim_start();
                    // In fact, it should not be Err if the above code is correct.
                    String::from_utf8(quoted_string)?
                } else {
                    // token: won't contains semicolon according to RFC 2616 Section 2.2
                    let (token, new_left) = split_once_and_trim(left, ';');
                    left = new_left;
                    if token.is_empty() {
                        // quoted-string can be empty, but token cannot be empty
                        anyhow::bail!("empty token");
                    }
                    token.to_owned()
                };

                let param = if param_name.eq_ignore_ascii_case("name") {
                    DispositionParam::Name(value)
                } else if param_name.eq_ignore_ascii_case("filename") {
                    // See also comments in test_from_raw_unnecessary_percent_decode.
                    DispositionParam::Filename(value)
                } else {
                    DispositionParam::Unknown(param_name.to_owned(), value)
                };
                cd.parameters.push(param);
            }
        }

        Ok(cd)
    }

    /// Returns `true` if type is [`Inline`](DispositionType::Inline).
    #[must_use]
    pub const fn is_inline(&self) -> bool {
        matches!(self.disposition, DispositionType::Inline)
    }

    /// Returns `true` if type is [`Attachment`](DispositionType::Attachment).
    #[must_use]
    pub const fn is_attachment(&self) -> bool {
        matches!(self.disposition, DispositionType::Attachment)
    }

    /// Returns `true` if type is [`FormData`](DispositionType::FormData).
    #[must_use]
    pub const fn is_form_data(&self) -> bool {
        matches!(self.disposition, DispositionType::FormData)
    }

    /// Returns `true` if type is [`Ext`](DispositionType::Ext) and the `disp_type` matches.
    pub fn is_ext(&self, disp_type: impl AsRef<str>) -> bool {
        matches!(
            self.disposition,
            DispositionType::Ext(ref t) if t.eq_ignore_ascii_case(disp_type.as_ref())
        )
    }

    /// Return the value of *name* if exists.
    pub fn get_name(&self) -> Option<&str> {
        self.parameters.iter().find_map(DispositionParam::as_name)
    }

    /// Return the value of *filename* if exists.
    pub fn get_filename(&self) -> Option<&str> {
        self.parameters
            .iter()
            .find_map(DispositionParam::as_filename)
    }

    /// Return the value of *filename\** if exists.
    pub fn get_filename_ext(&self) -> Option<&ExtendedValue> {
        self.parameters
            .iter()
            .find_map(DispositionParam::as_filename_ext)
    }

    /// Return the value of the parameter which the `name` matches.
    pub fn get_unknown(&self, name: impl AsRef<str>) -> Option<&str> {
        let name = name.as_ref();
        self.parameters.iter().find_map(|p| p.as_unknown(name))
    }

    /// Return the value of the extended parameter which the `name` matches.
    pub fn get_unknown_ext(&self, name: impl AsRef<str>) -> Option<&ExtendedValue> {
        let name = name.as_ref();
        self.parameters.iter().find_map(|p| p.as_unknown_ext(name))
    }
}

impl fmt::Display for DispositionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inline => write!(f, "inline"),
            Self::Attachment => write!(f, "attachment"),
            Self::FormData => write!(f, "form-data"),
            Self::Ext(s) => write!(f, "{}", s),
        }
    }
}

impl fmt::Display for DispositionParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // All ASCII control characters (0-30, 127) including horizontal tab, double quote, and
        // backslash should be escaped in quoted-string (i.e. "foobar").
        //
        // Ref: RFC 6266 §4.1 -> RFC 2616 §3.6
        //
        // filename-parm  = "filename" "=" value
        // value          = token | quoted-string
        // quoted-string  = ( <"> *(qdtext | quoted-pair ) <"> )
        // qdtext         = <any TEXT except <">>
        // quoted-pair    = "\" CHAR
        // TEXT           = <any OCTET except CTLs,
        //                  but including LWS>
        // LWS            = [CRLF] 1*( SP | HT )
        // OCTET          = <any 8-bit sequence of data>
        // CHAR           = <any US-ASCII character (octets 0 - 127)>
        // CTL            = <any US-ASCII control character
        //                  (octets 0 - 31) and DEL (127)>
        //
        // Ref: RFC 7578 S4.2 -> RFC 2183 S2 -> RFC 2045 S5.1
        // parameter := attribute "=" value
        // attribute := token
        //              ; Matching of attributes
        //              ; is ALWAYS case-insensitive.
        // value := token / quoted-string
        // token := 1*<any (US-ASCII) CHAR except SPACE, CTLs,
        //             or tspecials>
        // tspecials :=  "(" / ")" / "<" / ">" / "@" /
        //               "," / ";" / ":" / "\" / <">
        //               "/" / "[" / "]" / "?" / "="
        //               ; Must be in quoted-string,
        //               ; to use within parameter values
        //
        //
        // See also comments in test_from_raw_unnecessary_percent_decode.

        static RE: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new("[\x00-\x08\x10-\x1F\x7F\"\\\\]").expect("Regex shouldn't fail")
        });

        match self {
            Self::Name(value) => write!(f, "name={}", value),

            Self::Filename(value) => {
                write!(f, "filename=\"{}\"", RE.replace_all(value, "\\$0").as_ref())
            }

            Self::Unknown(name, value) => write!(
                f,
                "{}=\"{}\"",
                name,
                &RE.replace_all(value, "\\$0").as_ref()
            ),

            Self::FilenameExt(ext_value) => {
                write!(f, "filename*={}", ext_value)
            }

            Self::UnknownExt(name, ext_value) => {
                write!(f, "{}*={}", name, ext_value)
            }
        }
    }
}

impl fmt::Display for ContentDisposition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.disposition)?;
        self.parameters
            .iter()
            .try_for_each(|param| write!(f, "; {}", param))
    }
}
