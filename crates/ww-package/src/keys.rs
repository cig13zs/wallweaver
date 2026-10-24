use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use ed25519_dalek::{Signer as _, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::PackageError;

const SECRET_HEADER: &str = "wallweaver-secret-key v1";
const PUBLIC_HEADER: &str = "wallweaver-public-key v1";

/// Short fingerprint of a public key: the first 8 bytes of its SHA-256,
/// written as 16 uppercase hex digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyId([u8; 8]);

impl KeyId {
    fn of(public: &[u8; 32]) -> KeyId {
        let digest = Sha256::digest(public);
        let mut id = [0u8; 8];
        id.copy_from_slice(&digest[..8]);
        KeyId(id)
    }

    /// Parse 16 hex digits.
    pub fn parse(s: &str) -> Result<KeyId, PackageError> {
        let s = s.trim();
        if s.len() != 16 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(PackageError::Key(format!("'{s}' is not a 16-digit key id")));
        }
        let mut id = [0u8; 8];
        for (i, byte) in id.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
                .map_err(|_| PackageError::Key(format!("'{s}' is not a key id")))?;
        }
        Ok(KeyId(id))
    }

    /// Abbreviated form for UI, such as `7F3A…C21E`.
    pub fn short(&self) -> String {
        let full = self.to_string();
        format!("{}…{}", &full[..4], &full[12..])
    }
}

impl fmt::Display for KeyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in self.0 {
            write!(f, "{b:02X}")?;
        }
        Ok(())
    }
}

/// An ed25519 public key used to verify packages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKey {
    key: VerifyingKey,
}

impl PublicKey {
    pub(crate) fn from_bytes(bytes: &[u8; 32]) -> Result<PublicKey, PackageError> {
        VerifyingKey::from_bytes(bytes)
            .map(|key| PublicKey { key })
            .map_err(|_| PackageError::Key("not a valid ed25519 public key".into()))
    }

    /// Raw 32-byte key.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.key.to_bytes()
    }

    /// Base64 of the raw key.
    pub fn to_base64(&self) -> String {
        B64.encode(self.to_bytes())
    }

    /// Parse the base64 form produced by [`PublicKey::to_base64`].
    pub fn from_base64(s: &str) -> Result<PublicKey, PackageError> {
        let bytes = decode_fixed::<32>(s, "public key")?;
        PublicKey::from_bytes(&bytes)
    }

    /// Fingerprint of this key.
    pub fn key_id(&self) -> KeyId {
        KeyId::of(&self.to_bytes())
    }

    /// Strict ed25519 verification (rejects small-order and malleable forms).
    pub(crate) fn verify(&self, message: &[u8], signature: &[u8; 64]) -> bool {
        let sig = ed25519_dalek::Signature::from_bytes(signature);
        self.key.verify_strict(message, &sig).is_ok()
    }

    /// The text form saved in `.wwpub` files.
    pub fn to_file_string(&self) -> String {
        format!(
            "{PUBLIC_HEADER}\nkey-id: {}\npublic-key: {}\n",
            self.key_id(),
            self.to_base64()
        )
    }

    /// Parse a `.wwpub` file. The key id line, if present, must match.
    pub fn from_file_string(text: &str) -> Result<PublicKey, PackageError> {
        let fields = parse_key_file(text, PUBLIC_HEADER)?;
        let key = PublicKey::from_base64(field(&fields, "public-key")?)?;
        if let Some(id) = fields.iter().find(|(k, _)| *k == "key-id")
            && KeyId::parse(id.1)? != key.key_id()
        {
            return Err(PackageError::Key(
                "key-id does not match the public key".into(),
            ));
        }
        Ok(key)
    }
}

/// An ed25519 signing key. Keep it out of package folders and version control.
pub struct SecretKey {
    key: SigningKey,
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecretKey")
            .field("key_id", &self.public_key().key_id())
            .finish_non_exhaustive()
    }
}

impl SecretKey {
    /// Generate a new key from the OS random number generator.
    pub fn generate() -> Result<SecretKey, PackageError> {
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed)
            .map_err(|e| PackageError::Key(format!("no secure randomness available: {e}")))?;
        let key = SigningKey::from_bytes(&seed);
        seed.fill(0);
        Ok(SecretKey { key })
    }

    /// Build a key from a 32-byte seed. Intended for tests and fuzzing.
    pub fn from_seed(seed: &[u8; 32]) -> SecretKey {
        SecretKey {
            key: SigningKey::from_bytes(seed),
        }
    }

    /// The matching public key.
    pub fn public_key(&self) -> PublicKey {
        PublicKey {
            key: self.key.verifying_key(),
        }
    }

    pub(crate) fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.key.sign(message).to_bytes()
    }

    /// The text form saved in `.wwkey` files. It is not encrypted; protect
    /// the file with OS permissions.
    pub fn to_file_string(&self) -> String {
        format!(
            "{SECRET_HEADER}\nkey-id: {}\nsecret-key: {}\n",
            self.public_key().key_id(),
            B64.encode(self.key.to_bytes())
        )
    }

    /// Parse a `.wwkey` file.
    pub fn from_file_string(text: &str) -> Result<SecretKey, PackageError> {
        let fields = parse_key_file(text, SECRET_HEADER)?;
        let seed = decode_fixed::<32>(field(&fields, "secret-key")?, "secret key")?;
        let key = SecretKey::from_seed(&seed);
        if let Some(id) = fields.iter().find(|(k, _)| *k == "key-id")
            && KeyId::parse(id.1)? != key.public_key().key_id()
        {
            return Err(PackageError::Key(
                "key-id does not match the secret key".into(),
            ));
        }
        Ok(key)
    }

    /// True if `text` starts like a secret key file. Used to keep keys out
    /// of packages.
    pub fn looks_like_key_file(text: &[u8]) -> bool {
        text.starts_with(SECRET_HEADER.as_bytes())
    }
}

fn parse_key_file<'a>(
    text: &'a str,
    header: &str,
) -> Result<Vec<(&'a str, &'a str)>, PackageError> {
    let mut lines = text.lines().map(str::trim_end).filter(|l| !l.is_empty());
    match lines.next() {
        Some(first) if first == header => {}
        _ => return Err(PackageError::Key(format!("expected a '{header}' file"))),
    }
    lines
        .map(|line| {
            line.split_once(": ")
                .ok_or_else(|| PackageError::Key(format!("unexpected line '{line}'")))
        })
        .collect()
}

fn field<'a>(fields: &[(&str, &'a str)], name: &str) -> Result<&'a str, PackageError> {
    fields
        .iter()
        .find(|(k, _)| *k == name)
        .map(|(_, v)| *v)
        .ok_or_else(|| PackageError::Key(format!("missing '{name}'")))
}

pub(crate) fn decode_fixed<const N: usize>(s: &str, what: &str) -> Result<[u8; N], PackageError> {
    let bytes = B64
        .decode(s.trim())
        .map_err(|_| PackageError::Key(format!("{what} is not valid base64")))?;
    bytes
        .try_into()
        .map_err(|_| PackageError::Key(format!("{what} has the wrong length")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_files_round_trip() {
        let sk = SecretKey::from_seed(&[7u8; 32]);
        let text = sk.to_file_string();
        assert!(SecretKey::looks_like_key_file(text.as_bytes()));
        let back = SecretKey::from_file_string(&text).unwrap();
        assert_eq!(back.public_key(), sk.public_key());

        let pk_text = sk.public_key().to_file_string();
        let pk = PublicKey::from_file_string(&pk_text).unwrap();
        assert_eq!(pk, sk.public_key());
        // CRLF line endings are fine.
        assert!(PublicKey::from_file_string(&pk_text.replace('\n', "\r\n")).is_ok());
    }

    #[test]
    fn mismatched_key_id_is_rejected() {
        let a = SecretKey::from_seed(&[1u8; 32]).public_key();
        let b = SecretKey::from_seed(&[2u8; 32]).public_key();
        let forged = a
            .to_file_string()
            .replace(&a.key_id().to_string(), &b.key_id().to_string());
        assert!(PublicKey::from_file_string(&forged).is_err());
    }

    #[test]
    fn wrong_header_and_garbage_rejected() {
        let sk = SecretKey::from_seed(&[3u8; 32]);
        assert!(PublicKey::from_file_string(&sk.to_file_string()).is_err());
        assert!(SecretKey::from_file_string("hello").is_err());
        assert!(PublicKey::from_base64("!!!").is_err());
        assert!(PublicKey::from_base64("AAAA").is_err());
    }

    #[test]
    fn key_id_format() {
        let id = SecretKey::from_seed(&[9u8; 32]).public_key().key_id();
        let s = id.to_string();
        assert_eq!(s.len(), 16);
        assert_eq!(KeyId::parse(&s).unwrap(), id);
        assert_eq!(id.short().chars().count(), 9);
        assert!(KeyId::parse("xyz").is_err());
    }

    #[test]
    fn generated_keys_differ() {
        let a = SecretKey::generate().unwrap();
        let b = SecretKey::generate().unwrap();
        assert_ne!(a.public_key(), b.public_key());
        let dbg = format!("{a:?}");
        assert!(
            !dbg.contains(&B64.encode(a.key.to_bytes())),
            "Debug must not leak the key"
        );
    }
}
