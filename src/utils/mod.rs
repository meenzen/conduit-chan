pub(crate) mod error;

use crate::{services, Error, Result};
use argon2::{password_hash::SaltString, PasswordHasher};
use rand::prelude::*;
use ring::digest;
use ruma::{
    canonical_json::try_from_json_map, CanonicalJsonError, CanonicalJsonObject, OwnedUserId,
};
use std::{
    cmp::Ordering,
    fmt,
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) fn millis_since_unix_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time is valid")
        .as_millis() as u64
}

pub(crate) fn increment(old: Option<&[u8]>) -> Option<Vec<u8>> {
    let number = match old.map(|bytes| bytes.try_into()) {
        Some(Ok(bytes)) => {
            let number = u64::from_be_bytes(bytes);
            number + 1
        }
        _ => 1, // Start at one. since 0 should return the first event in the db
    };

    Some(number.to_be_bytes().to_vec())
}

pub fn generate_keypair() -> Vec<u8> {
    let mut value = random_string(8).as_bytes().to_vec();
    value.push(0xff);
    value.extend_from_slice(
        &ruma::signatures::Ed25519KeyPair::generate()
            .expect("Ed25519KeyPair generation always works (?)"),
    );
    value
}

/// Parses the bytes into an u64.
pub fn u64_from_bytes(bytes: &[u8]) -> Result<u64, std::array::TryFromSliceError> {
    let array: [u8; 8] = bytes.try_into()?;
    Ok(u64::from_be_bytes(array))
}

/// Parses the bytes into a string.
pub fn string_from_bytes(bytes: &[u8]) -> Result<String, std::string::FromUtf8Error> {
    String::from_utf8(bytes.to_vec())
}

/// Parses a OwnedUserId from bytes.
pub fn user_id_from_bytes(bytes: &[u8]) -> Result<OwnedUserId> {
    OwnedUserId::try_from(
        string_from_bytes(bytes)
            .map_err(|_| Error::bad_database("Failed to parse string from bytes"))?,
    )
    .map_err(|_| Error::bad_database("Failed to parse user id from bytes"))
}

pub fn random_string(length: usize) -> String {
    thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

/// Calculate a new hash for the given password
pub fn calculate_password_hash(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(thread_rng());
    services()
        .globals
        .argon
        .hash_password(password.as_bytes(), &salt)
        .map(|it| it.to_string())
}

#[tracing::instrument(skip(keys))]
pub fn calculate_hash(keys: &[&[u8]]) -> Vec<u8> {
    // We only hash the pdu's event ids, not the whole pdu
    let bytes = keys.join(&0xff);
    let hash = digest::digest(&digest::SHA256, &bytes);
    hash.as_ref().to_owned()
}

pub(crate) fn common_elements(
    mut iterators: impl Iterator<Item = impl Iterator<Item = Vec<u8>>>,
    check_order: impl Fn(&[u8], &[u8]) -> Ordering,
) -> Option<impl Iterator<Item = Vec<u8>>> {
    let first_iterator = iterators.next()?;
    let mut other_iterators = iterators.map(|i| i.peekable()).collect::<Vec<_>>();

    Some(first_iterator.filter(move |target| {
        other_iterators.iter_mut().all(|it| {
            while let Some(element) = it.peek() {
                match check_order(element, target) {
                    Ordering::Greater => return false, // We went too far
                    Ordering::Equal => return true,    // Element is in both iters
                    Ordering::Less => {
                        // Keep searching
                        it.next();
                    }
                }
            }
            false
        })
    }))
}

/// Fallible conversion from any value that implements `Serialize` to a `CanonicalJsonObject`.
///
/// `value` must serialize to an `serde_json::Value::Object`.
pub(crate) fn to_canonical_object<T: serde::Serialize>(
    value: T,
) -> Result<CanonicalJsonObject, CanonicalJsonError> {
    use serde::ser::Error;

    match serde_json::to_value(value).map_err(CanonicalJsonError::SerDe)? {
        serde_json::Value::Object(map) => try_from_json_map(map),
        _ => Err(CanonicalJsonError::SerDe(serde_json::Error::custom(
            "Value must be an object",
        ))),
    }
}

pub(crate) fn deserialize_from_str<
    'de,
    D: serde::de::Deserializer<'de>,
    T: FromStr<Err = E>,
    E: std::fmt::Display,
>(
    deserializer: D,
) -> Result<T, D::Error> {
    struct Visitor<T: FromStr<Err = E>, E>(std::marker::PhantomData<T>);
    impl<'de, T: FromStr<Err = Err>, Err: std::fmt::Display> serde::de::Visitor<'de>
        for Visitor<T, Err>
    {
        type Value = T;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(formatter, "a parsable string")
        }
        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            v.parse().map_err(serde::de::Error::custom)
        }
    }
    deserializer.deserialize_str(Visitor(std::marker::PhantomData))
}

// Copied from librustdoc:
// https://github.com/rust-lang/rust/blob/cbaeec14f90b59a91a6b0f17fc046c66fa811892/src/librustdoc/html/escape.rs

/// Wrapper struct which will emit the HTML-escaped version of the contained
/// string when passed to a format string.
pub(crate) struct HtmlEscape<'a>(pub(crate) &'a str);

impl<'a> fmt::Display for HtmlEscape<'a> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Because the internet is always right, turns out there's not that many
        // characters to escape: http://stackoverflow.com/questions/7381974
        let HtmlEscape(s) = *self;
        let pile_o_bits = s;
        let mut last = 0;
        for (i, ch) in s.char_indices() {
            let s = match ch {
                '>' => "&gt;",
                '<' => "&lt;",
                '&' => "&amp;",
                '\'' => "&#39;",
                '"' => "&quot;",
                _ => continue,
            };
            fmt.write_str(&pile_o_bits[last..i])?;
            fmt.write_str(s)?;
            // NOTE: we only expect single byte characters here - which is fine as long as we
            // only match single byte characters
            last = i + 1;
        }

        if last < s.len() {
            fmt.write_str(&pile_o_bits[last..])?;
        }
        Ok(())
    }
}
