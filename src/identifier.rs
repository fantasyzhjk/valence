use std::borrow::Cow;
use std::io::{Read, Write};
use std::str::FromStr;

use ascii::{AsAsciiStr, AsciiChar, AsciiStr, IntoAsciiString};
use serde::de::Visitor;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::protocol::{encode_string_bounded, BoundedString, Decode, Encode};

/// An identifier is a string split into a "namespace" part and a "name" part.
/// For instance `minecraft:apple` and `apple` are both valid identifiers.
///
/// If the namespace part is left off (the part before and including the colon)
/// the namespace is considered to be "minecraft".
///
/// The entire identifier must match the regex `([a-z0-9_-]+:)?[a-z0-9_\/.-]+`.
#[derive(Clone, Eq)]
pub struct Identifier {
    ident: Cow<'static, AsciiStr>,
    /// The index of the ':' character in the string.
    /// If there is no namespace then it is `usize::MAX`.
    ///
    /// Since the string only contains ASCII characters, we can slice it
    /// in O(1) time.
    colon_idx: usize,
}

#[derive(Clone, Error, PartialEq, Eq, Debug)]
#[error("invalid identifier \"{src}\"")]
pub struct ParseError {
    src: Cow<'static, str>,
}

impl Identifier {
    /// Parses a new identifier from a string.
    ///
    /// The string must match the regex `([a-z0-9_-]+:)?[a-z0-9_\/.-]+`.
    /// If not, an error is returned.
    pub fn new(str: impl Into<Cow<'static, str>>) -> Result<Identifier, ParseError> {
        #![allow(bindings_with_variant_name)]

        let cow = match str.into() {
            Cow::Borrowed(s) => {
                Cow::Borrowed(s.as_ascii_str().map_err(|_| ParseError { src: s.into() })?)
            }
            Cow::Owned(s) => Cow::Owned(s.into_ascii_string().map_err(|e| ParseError {
                src: e.into_source().into(),
            })?),
        };

        let s = cow.as_ref();

        let check_namespace = |s: &AsciiStr| {
            !s.is_empty()
                && s.chars()
                    .all(|c| matches!(c.as_char(), 'a'..='z' | '0'..='9' | '_' | '-'))
        };
        let check_name = |s: &AsciiStr| {
            !s.is_empty()
                && s.chars()
                    .all(|c| matches!(c.as_char(), 'a'..='z' | '0'..='9' | '_' | '/' | '.' | '-'))
        };

        if let Some(colon_idx) = s.chars().position(|c| c == AsciiChar::Colon) {
            if check_namespace(&s[..colon_idx]) && check_name(&s[colon_idx + 1..]) {
                Ok(Self {
                    ident: cow,
                    colon_idx,
                })
            } else {
                Err(ParseError {
                    src: ascii_cow_to_str_cow(cow),
                })
            }
        } else if check_name(s) {
            Ok(Self {
                ident: cow,
                colon_idx: usize::MAX,
            })
        } else {
            Err(ParseError {
                src: ascii_cow_to_str_cow(cow),
            })
        }
    }

    /// Returns the namespace part of this namespaced identifier.
    /// If this identifier was constructed from a string without a namespace,
    /// then `None` is returned.
    pub fn namespace(&self) -> Option<&str> {
        if self.colon_idx == usize::MAX {
            None
        } else {
            Some(self.ident[..self.colon_idx].as_str())
        }
    }

    /// Returns the name part of this namespaced identifier.
    pub fn name(&self) -> &str {
        if self.colon_idx == usize::MAX {
            self.ident.as_str()
        } else {
            self.ident[self.colon_idx + 1..].as_str()
        }
    }

    /// Returns the identifier as a `str`.
    pub fn as_str(&self) -> &str {
        self.ident.as_str()
    }
}

fn ascii_cow_to_str_cow(cow: Cow<AsciiStr>) -> Cow<str> {
    match cow {
        Cow::Borrowed(s) => Cow::Borrowed(s.as_str()),
        Cow::Owned(s) => Cow::Owned(s.into()),
    }
}

impl ParseError {
    pub fn into_source(self) -> Cow<'static, str> {
        self.src
    }
}

impl std::fmt::Debug for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Identifier").field(&self.as_str()).finish()
    }
}

impl FromStr for Identifier {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Identifier::new(s.to_string())
    }
}

impl From<Identifier> for String {
    fn from(id: Identifier) -> Self {
        id.ident.into_owned().into()
    }
}

impl From<Identifier> for Cow<'static, str> {
    fn from(id: Identifier) -> Self {
        ascii_cow_to_str_cow(id.ident)
    }
}

impl AsRef<str> for Identifier {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl TryFrom<String> for Identifier {
    type Error = ParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Identifier::new(value)
    }
}

impl TryFrom<&'static str> for Identifier {
    type Error = ParseError;

    fn try_from(value: &'static str) -> Result<Self, Self::Error> {
        Identifier::new(value)
    }
}

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Equality for identifiers respects the fact that "minecraft:apple" and
/// "apple" have the same meaning.
impl PartialEq for Identifier {
    fn eq(&self, other: &Self) -> bool {
        self.namespace().unwrap_or("minecraft") == other.namespace().unwrap_or("minecraft")
            && self.name() == other.name()
    }
}

impl std::hash::Hash for Identifier {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.namespace().unwrap_or("minecraft").hash(state);
        self.name().hash(state);
    }
}

impl Encode for Identifier {
    fn encode(&self, w: &mut impl Write) -> anyhow::Result<()> {
        encode_string_bounded(self.as_str(), 0, 32767, w)
    }
}

impl Decode for Identifier {
    fn decode(r: &mut impl Read) -> anyhow::Result<Self> {
        let string = BoundedString::<0, 32767>::decode(r)?.0;
        Ok(Identifier::new(string)?)
    }
}

impl Serialize for Identifier {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.as_str().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Identifier {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(IdentifierVisitor)
    }
}

/// An implementation of `serde::de::Visitor` for Minecraft identifiers.
struct IdentifierVisitor;

impl<'de> Visitor<'de> for IdentifierVisitor {
    type Value = Identifier;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "a valid Minecraft identifier")
    }

    fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Self::Value, E> {
        Identifier::from_str(s).map_err(E::custom)
    }

    fn visit_string<E: serde::de::Error>(self, s: String) -> Result<Self::Value, E> {
        Identifier::new(s).map_err(E::custom)
    }
}

/// Convenience macro for constructing an identifier from a format string.
///
/// The macro will panic if the formatted string is not a valid
/// identifier.
#[macro_export]
macro_rules! ident {
    ($($arg:tt)*) => {{
        let errmsg = "invalid identifier in `ident` macro";
        #[allow(clippy::redundant_closure_call)]
        (|args: ::std::fmt::Arguments| match args.as_str() {
            Some(s) => $crate::Identifier::new(s).expect(errmsg),
            None => $crate::Identifier::new(args.to_string()).expect(errmsg),
        })(format_args!($($arg)*))
    }}
}

#[cfg(test)]
mod tests {
    #[test]
    fn parse_valid() {
        ident!("minecraft:whatever");
        ident!("_what-ever55_:.whatever/whatever123456789_");
    }

    #[test]
    #[should_panic]
    fn parse_invalid_0() {
        ident!("");
    }

    #[test]
    #[should_panic]
    fn parse_invalid_1() {
        ident!(":");
    }

    #[test]
    #[should_panic]
    fn parse_invalid_2() {
        ident!("foo:bar:baz");
    }

    #[test]
    fn equality() {
        assert_eq!(ident!("minecraft:my.identifier"), ident!("my.identifier"));
    }
}