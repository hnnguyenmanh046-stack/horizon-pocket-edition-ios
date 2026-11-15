//! A shared world's invite: one line of text that holds where the world's bucket is, the key
//! that opens it, and which world in it. Whoever has the line is a member of the world, so it
//! is sent to friends only; it is not the short-lived code that joins a host.
use crate::{
    Error, Result,
    codec::{base64, hex, random, sha256, unbase64},
    s3::Bucket,
};
use serde::{Deserialize, Serialize};

/// What every invite starts with: the format and its version. Version 2 packs the bucket's
/// address and keys as bytes (about a third the length of version 1's text); version 1 invites
/// still read.
pub const PREFIX: &str = "HSW2.";
const PREFIX_V1: &str = "HSW1.";

/// R2's S3 endpoint, `https://<account>.r2.cloudflarestorage.com` (or a jurisdiction's, `.eu.`):
/// the account's 16 bytes and whether it is the EU one.
fn r2_account(endpoint: &str) -> Option<([u8; 16], bool)> {
    let host = endpoint.strip_prefix("https://")?;
    let (account, eu) = match host.strip_suffix(".eu.r2.cloudflarestorage.com") {
        Some(account) => (account, true),
        None => (host.strip_suffix(".r2.cloudflarestorage.com")?, false),
    };
    lower_hex::<16>(account).map(|a| (a, eu))
}

/// `text` as N bytes when it is exactly 2N lowercase hex digits (so it reads back the same).
fn lower_hex<const N: usize>(text: &str) -> Option<[u8; N]> {
    if text.len() != 2 * N || !text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return None;
    }
    crate::codec::unhex(text)?.try_into().ok()
}

const R2: u8 = 1;
const R2_EU: u8 = 2;
const AUTO: u8 = 4;
const KEY_HEX: u8 = 8;
const SECRET_HEX: u8 = 16;

fn put_text(out: &mut Vec<u8>, text: &str) -> Result<()> {
    let len = u16::try_from(text.len()).map_err(|_| Error::Format("too long for an invite".into()))?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(text.as_bytes());
    Ok(())
}

/// Reads an invite's packed bytes in order.
struct Reader<'a>(&'a [u8]);
impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        if self.0.len() < n {
            return Err(Error::Format("the invite is incomplete: copy the whole line".into()));
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(head)
    }
    fn text(&mut self) -> Result<String> {
        let len = u16::from_be_bytes(self.take(2)?.try_into().unwrap_or_default());
        String::from_utf8(self.take(usize::from(len))?.to_vec())
            .map_err(|_| Error::Format("the invite's text doesn't read".into()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldInvite {
    pub bucket: Bucket,
    /// The world's id: 16 hex digits, the name of its folder in the bucket.
    pub world: String,
    /// The world's name as its players see it.
    pub name: String,
}

fn valid_id(id: &str) -> bool {
    id.len() == 16 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

impl WorldInvite {
    /// A new world's id.
    pub fn new_id() -> Result<String> {
        Ok(hex(&random::<8>()?))
    }

    /// The world's folder in the bucket: `w/<id>/`.
    pub fn prefix(&self) -> String {
        format!("w/{}/", self.world)
    }

    fn check(&self) -> Result<()> {
        if !valid_id(&self.world) {
            return Err(Error::Format("the invite names no world".into()));
        }
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
            return Err(Error::Format(
                "the invite's world has no usable name".into(),
            ));
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<String> {
        self.check()?;
        let b = &self.bucket;
        let account = r2_account(&b.endpoint);
        let key = lower_hex::<16>(&b.access_key);
        let secret = lower_hex::<32>(&b.secret_key);
        let mut flags = 0u8;
        if let Some((_, eu)) = account {
            flags |= if eu { R2_EU } else { R2 };
        }
        if b.region == "auto" {
            flags |= AUTO;
        }
        if key.is_some() {
            flags |= KEY_HEX;
        }
        if secret.is_some() {
            flags |= SECRET_HEX;
        }
        let mut data = vec![flags];
        match account {
            Some((account, _)) => data.extend_from_slice(&account),
            None => put_text(&mut data, &b.endpoint)?,
        }
        put_text(&mut data, &b.bucket)?;
        if flags & AUTO == 0 {
            put_text(&mut data, &b.region)?;
        }
        match key {
            Some(key) => data.extend_from_slice(&key),
            None => put_text(&mut data, &b.access_key)?,
        }
        match secret {
            Some(secret) => data.extend_from_slice(&secret),
            None => put_text(&mut data, &b.secret_key)?,
        }
        data.extend_from_slice(
            &crate::codec::unhex(&self.world)
                .ok_or_else(|| Error::Format("the invite names no world".into()))?,
        );
        put_text(&mut data, self.name.trim())?;
        let sum = sha256(&data);
        data.extend_from_slice(&sum[..4]);
        Ok(format!("{PREFIX}{}", base64(&data)))
    }

    fn unpack(data: &[u8]) -> Result<Self> {
        let mut r = Reader(data);
        let flags = r.take(1)?[0];
        let endpoint = if flags & (R2 | R2_EU) != 0 {
            let eu = if flags & R2_EU != 0 { ".eu" } else { "" };
            format!("https://{}{eu}.r2.cloudflarestorage.com", hex(r.take(16)?))
        } else {
            r.text()?
        };
        let bucket = r.text()?;
        let region = if flags & AUTO != 0 {
            "auto".to_owned()
        } else {
            r.text()?
        };
        let access_key = if flags & KEY_HEX != 0 {
            hex(r.take(16)?)
        } else {
            r.text()?
        };
        let secret_key = if flags & SECRET_HEX != 0 {
            hex(r.take(32)?)
        } else {
            r.text()?
        };
        let world = hex(r.take(8)?);
        let name = r.text()?;
        if !r.0.is_empty() {
            return Err(Error::Format("the invite has more than it should".into()));
        }
        Ok(Self {
            bucket: Bucket {
                endpoint,
                bucket,
                region,
                access_key,
                secret_key,
            },
            world,
            name,
        })
    }

    /// Reads an invite as pasted: spaces and line breaks a chat app added are ignored, and a
    /// line that was cut or mistyped is caught by its checksum.
    pub fn decode(text: &str) -> Result<Self> {
        let text: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let (body, packed) = match (text.strip_prefix(PREFIX), text.strip_prefix(PREFIX_V1)) {
            (Some(body), _) => (body, true),
            (None, Some(body)) => (body, false),
            (None, None) => return Err(Error::Format("this is not a shared world invite".into())),
        };
        let cut = || Error::Format("the invite is incomplete: copy the whole line".into());
        let data = unbase64(body).map_err(|_| cut())?;
        let Some((content, sum)) = data.split_last_chunk::<4>() else {
            return Err(cut());
        };
        if sha256(content)[..4] != sum[..] {
            return Err(cut());
        }
        let invite: Self = if packed {
            Self::unpack(content)?
        } else {
            serde_json::from_slice(content)?
        };
        invite.check()?;
        Ok(invite)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invite() -> WorldInvite {
        WorldInvite {
            bucket: Bucket {
                endpoint: "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com"
                    .into(),
                bucket: "our-worlds".into(),
                region: "auto".into(),
                access_key: "0123456789abcdef0123456789abcdef".into(),
                secret_key: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .into(),
            },
            world: WorldInvite::new_id().unwrap(),
            name: "The Island".into(),
        }
    }

    /// As version 1 wrote an invite: its JSON and checksum.
    fn encode_v1(invite: &WorldInvite) -> String {
        let mut data = serde_json::to_vec(invite).unwrap();
        let sum = sha256(&data);
        data.extend_from_slice(&sum[..4]);
        format!("{PREFIX_V1}{}", base64(&data))
    }

    #[test]
    fn an_invite_is_short_and_one_from_before_still_reads() {
        let invite = invite();
        let text = invite.encode().unwrap();
        let old = encode_v1(&invite);
        assert!(text.len() <= 150, "{} characters: {text}", text.len());
        assert!(text.len() * 2 < old.len(), "{} against {}", text.len(), old.len());
        assert_eq!(WorldInvite::decode(&old).unwrap(), invite);
        // A bucket elsewhere (a jurisdiction, another S3 storage, keys that aren't hex, a
        // region) reads back the same too.
        let mut other = invite.clone();
        other.bucket.endpoint =
            "https://0123456789abcdef0123456789abcdef.eu.r2.cloudflarestorage.com".into();
        assert_eq!(WorldInvite::decode(&other.encode().unwrap()).unwrap(), other);
        other.bucket.endpoint = "https://s3.eu-central-1.wasabisys.com".into();
        other.bucket.region = "eu-central-1".into();
        other.bucket.access_key = "AKIAIOSFODNN7EXAMPLE".into();
        other.bucket.secret_key = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into();
        other.name = "Île ☀ à deux".into();
        assert_eq!(WorldInvite::decode(&other.encode().unwrap()).unwrap(), other);
        // Uppercase hex isn't packed (it would read back lowercase).
        other.bucket.access_key = "0123456789ABCDEF0123456789ABCDEF".into();
        assert_eq!(WorldInvite::decode(&other.encode().unwrap()).unwrap(), other);
    }

    #[test]
    fn an_invite_reads_back_however_a_chat_wrapped_it() {
        let invite = invite();
        let text = invite.encode().unwrap();
        assert!(text.starts_with(PREFIX));
        assert!(text.len() < 600, "{} characters", text.len());
        assert_eq!(WorldInvite::decode(&text).unwrap(), invite);
        let wrapped: String = text
            .as_bytes()
            .chunks(40)
            .map(|line| format!("  {}\r\n", std::str::from_utf8(line).unwrap()))
            .collect();
        assert_eq!(WorldInvite::decode(&wrapped).unwrap(), invite);
        assert_eq!(invite.prefix(), format!("w/{}/", invite.world));
    }

    #[test]
    fn a_cut_or_changed_invite_is_refused() {
        let text = invite().encode().unwrap();
        assert!(WorldInvite::decode(&text[..text.len() - 3]).is_err());
        assert!(WorldInvite::decode(&text[PREFIX.len()..]).is_err());
        assert!(WorldInvite::decode("").is_err());
        assert!(WorldInvite::decode(PREFIX).is_err());
        // One character changed in the middle.
        let mut changed = text.clone().into_bytes();
        let middle = changed.len() / 2;
        changed[middle] = if changed[middle] == b'A' { b'B' } else { b'A' };
        assert!(WorldInvite::decode(std::str::from_utf8(&changed).unwrap()).is_err());
    }

    #[test]
    fn an_invite_without_a_world_or_a_name_is_not_made() {
        let mut bad = invite();
        bad.world = "../lease".into();
        assert!(bad.encode().is_err());
        let mut bad = invite();
        bad.name = "  ".into();
        assert!(bad.encode().is_err());
        let ids: std::collections::HashSet<String> =
            (0..50).map(|_| WorldInvite::new_id().unwrap()).collect();
        assert_eq!(ids.len(), 50);
        assert!(ids.iter().all(|id| valid_id(id)));
    }
}
