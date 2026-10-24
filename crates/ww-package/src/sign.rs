use std::io::{Read, Seek, Write};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use crate::keys::decode_fixed;
use crate::read::{Opened, Signer, VerifiedPackage, VerifyOptions};
use crate::{KeyId, PackageError, PublicKey, SIGNATURE_PATH, SecretKey};

const HEADER: &str = "wallweaver-signature v1";
/// Domain separation prefix, so a package signature can never be replayed
/// as a signature over some other kind of message.
const CONTEXT: &[u8] = b"wallweaver-wwp-v1\n";

pub(crate) fn signed_message(checksums: &[u8]) -> Vec<u8> {
    let mut msg = Vec::with_capacity(CONTEXT.len() + checksums.len());
    msg.extend_from_slice(CONTEXT);
    msg.extend_from_slice(checksums);
    msg
}

/// Parsed `.wwp/SIGNATURE`.
pub(crate) struct Signature {
    pub public_key: PublicKey,
    pub signature: [u8; 64],
}

impl Signature {
    pub fn render(&self) -> String {
        format!(
            "{HEADER}\nkey-id: {}\npublic-key: {}\nsignature: {}\n",
            self.public_key.key_id(),
            self.public_key.to_base64(),
            B64.encode(self.signature)
        )
    }

    pub fn parse(bytes: &[u8]) -> Result<Signature, PackageError> {
        let bad = |m: &str| PackageError::MalformedSignature(m.to_string());
        let text = std::str::from_utf8(bytes).map_err(|_| bad("not UTF-8"))?;
        let lines: Vec<&str> = text.split_terminator('\n').collect();
        let [header, key_id, public_key, signature] = lines.as_slice() else {
            return Err(bad("expected four lines"));
        };
        if *header != HEADER {
            return Err(bad("unknown header"));
        }
        let key_id = key_id
            .strip_prefix("key-id: ")
            .ok_or_else(|| bad("missing key-id"))?;
        let public_key = public_key
            .strip_prefix("public-key: ")
            .ok_or_else(|| bad("missing public-key"))?;
        let signature = signature
            .strip_prefix("signature: ")
            .ok_or_else(|| bad("missing signature"))?;
        let public_key =
            PublicKey::from_base64(public_key).map_err(|_| bad("public-key is not valid"))?;
        let signature = decode_fixed::<64>(signature, "signature")
            .map_err(|_| bad("signature is not valid"))?;
        let key_id = KeyId::parse(key_id).map_err(|_| bad("key-id is not valid"))?;
        if key_id != public_key.key_id() {
            return Err(bad("key-id does not match public-key"));
        }
        let sig = Signature {
            public_key,
            signature,
        };
        if sig.render().as_bytes() != bytes {
            return Err(bad("not in canonical form"));
        }
        Ok(sig)
    }
}

/// Sign a package: verify it completely, then write a copy with
/// `.wwp/SIGNATURE` added (replacing any earlier signature).
///
/// File entries are copied byte for byte, so signing never changes the
/// package content.
pub fn sign_package<R: Read + Seek, W: Write + Seek>(
    input: R,
    output: W,
    key: &SecretKey,
) -> Result<VerifiedPackage, PackageError> {
    let mut opened = Opened::open(input, &VerifyOptions::developer())?;
    opened.verify_all()?;

    let checksums_text = opened.checksums.render();
    let sig = Signature {
        public_key: key.public_key(),
        signature: key.sign(&signed_message(checksums_text.as_bytes())),
    };

    let mut zip = ZipWriter::new(output);
    let checksums_entry = opened
        .archive
        .by_index_raw(opened.checksums_index)
        .map_err(PackageError::zip)?;
    zip.raw_copy_file(checksums_entry)
        .map_err(PackageError::zip)?;

    zip.start_file(
        SIGNATURE_PATH,
        SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .last_modified_time(DateTime::DEFAULT)
            .unix_permissions(0o644),
    )
    .map_err(PackageError::zip)?;
    zip.write_all(sig.render().as_bytes())
        .map_err(|e| PackageError::io("writing signature", e))?;

    let paths: Vec<String> = opened
        .checksums
        .entries()
        .iter()
        .map(|e| e.path.clone())
        .collect();
    for path in &paths {
        let index = opened.files[path];
        let entry = opened
            .archive
            .by_index_raw(index)
            .map_err(PackageError::zip)?;
        zip.raw_copy_file(entry).map_err(PackageError::zip)?;
    }
    zip.finish().map_err(PackageError::zip)?;

    let public_key = key.public_key();
    opened.signer = Some(Signer {
        key_id: public_key.key_id(),
        public_key,
    });
    Ok(opened.into_verified())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_file_round_trips_and_is_strict() {
        let key = SecretKey::from_seed(&[5u8; 32]);
        let sig = Signature {
            public_key: key.public_key(),
            signature: key.sign(b"hello"),
        };
        let text = sig.render();
        let back = Signature::parse(text.as_bytes()).unwrap();
        assert_eq!(back.signature, sig.signature);
        assert!(Signature::parse(text.replace('\n', "\r\n").as_bytes()).is_err());
        assert!(Signature::parse(format!("{text}extra: 1\n").as_bytes()).is_err());
        let other = SecretKey::from_seed(&[6u8; 32]).public_key();
        let swapped = text.replace(&key.public_key().to_base64(), &other.to_base64());
        assert!(
            Signature::parse(swapped.as_bytes()).is_err(),
            "key-id must match"
        );
    }

    #[test]
    fn context_prefix_is_applied() {
        let msg = signed_message(b"abc");
        assert!(msg.starts_with(CONTEXT));
        assert!(msg.ends_with(b"abc"));
    }
}
