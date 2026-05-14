//! Drop-zeroize secret bytes; masks Debug and Serialize by default.

use std::fmt;

use serde::{de::Visitor, Deserialize, Deserializer, Serialize, Serializer};
use zeroize::Zeroize;

pub struct Secret(Box<[u8]>);

impl Secret {
    pub fn new() -> Self {
        Self(Box::from([]))
    }

    pub fn from_string(mut s: String) -> Self {
        let bytes = s.as_bytes().to_vec().into_boxed_slice();
        s.zeroize();
        Self(bytes)
    }

    pub fn from_bytes(b: Vec<u8>) -> Self {
        Self(b.into_boxed_slice())
    }

    pub fn reveal(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or("")
    }

    pub fn reveal_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn clear(&mut self) {
        let mut taken = std::mem::replace(&mut self.0, Box::from([]));
        taken.zeroize();
    }

    pub fn push_char(&mut self, c: char) {
        let mut buf = [0u8; 4];
        let s = c.encode_utf8(&mut buf);
        let mut v: Vec<u8> = self.0.to_vec();
        v.extend_from_slice(s.as_bytes());
        let mut old = std::mem::replace(&mut self.0, v.into_boxed_slice());
        old.zeroize();
    }

    pub fn pop_char(&mut self) {
        let idx_opt = std::str::from_utf8(&self.0)
            .ok()
            .and_then(|s| s.char_indices().next_back().map(|(i, _)| i));
        if let Some(idx) = idx_opt {
            let v: Vec<u8> = self.0[..idx].to_vec();
            let mut old = std::mem::replace(&mut self.0, v.into_boxed_slice());
            old.zeroize();
        }
    }
}

impl Default for Secret {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            f.write_str("Secret(\"\")")
        } else {
            f.write_str("Secret(***)")
        }
    }
}

impl Clone for Secret {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl PartialEq for Secret {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Secret {}

impl Serialize for Secret {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        let masked = if self.0.is_empty() { "" } else { "***" };
        ser.serialize_str(masked)
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Secret;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a string")
            }
            fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Secret, E> {
                Ok(Secret::from_string(s.to_string()))
            }
            fn visit_string<E: serde::de::Error>(self, s: String) -> Result<Secret, E> {
                Ok(Secret::from_string(s))
            }
        }
        de.deserialize_str(V)
    }
}

/// Wire-only serializer that writes the actual bytes; pair with `deserialize_secret`.
pub fn serialize_revealed<S: Serializer>(s: &Secret, ser: S) -> Result<S::Ok, S::Error> {
    ser.serialize_str(s.reveal())
}

/// Pair with `serialize_revealed`; normal deserialize already accepts plaintext.
pub fn deserialize_secret<'de, D: Deserializer<'de>>(de: D) -> Result<Secret, D::Error> {
    Secret::deserialize(de)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_masks_nonempty() {
        let s = Secret::from_string("hunter2".to_string());
        let d = format!("{:?}", s);
        assert!(!d.contains("hunter2"));
        assert!(d.contains("***"));
    }

    #[test]
    fn debug_empty_does_not_say_stars() {
        let s = Secret::new();
        let d = format!("{:?}", s);
        assert!(!d.contains("***"));
    }

    #[test]
    fn default_serialize_masks() {
        let s = Secret::from_string("hunter2".to_string());
        let j = serde_json::to_string(&s).unwrap();
        assert_eq!(j, "\"***\"");
    }

    #[test]
    fn default_serialize_empty_is_empty() {
        let s = Secret::new();
        let j = serde_json::to_string(&s).unwrap();
        assert_eq!(j, "\"\"");
    }

    #[test]
    fn deserialize_round_trips_plaintext() {
        let s: Secret = serde_json::from_str("\"hunter2\"").unwrap();
        assert_eq!(s.reveal(), "hunter2");
    }

    #[test]
    fn revealed_serializer_writes_plaintext() {
        #[derive(Serialize)]
        struct W<'a> {
            #[serde(serialize_with = "serialize_revealed")]
            p: &'a Secret,
        }
        let s = Secret::from_string("hunter2".to_string());
        let j = serde_json::to_string(&W { p: &s }).unwrap();
        assert_eq!(j, "{\"p\":\"hunter2\"}");
    }

    #[test]
    fn clone_does_not_share_storage() {
        let a = Secret::from_string("hunter2".to_string());
        let b = a.clone();
        assert_eq!(a.reveal(), b.reveal());
        assert_eq!(a, b);
    }

    #[test]
    fn push_and_pop_char_round_trip() {
        let mut s = Secret::new();
        s.push_char('h');
        s.push_char('i');
        assert_eq!(s.reveal(), "hi");
        s.pop_char();
        assert_eq!(s.reveal(), "h");
        s.clear();
        assert!(s.is_empty());
    }
}
