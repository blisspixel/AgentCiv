use crate::{Error, RawPage, Result, reflected};
use agentciv_archive::parse_unique;
use sha2::{Digest, Sha256};

pub(super) fn digest(records: &[String]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"agentciv-prefix/0.1\0");
    for raw in records {
        hash.update((raw.len() as u64).to_be_bytes());
        hash.update(raw.as_bytes());
    }
    format!("{:x}", hash.finalize())
}

pub(super) fn validate(bytes: &[u8], records: &mut Vec<String>, token: &str) -> Result<String> {
    let raw = std::str::from_utf8(bytes).map_err(|_| Error::InvalidUtf8)?;
    let page = parse_unique(raw).map_err(|_| Error::InvalidJson)?;
    if reflected(raw, &page, Some(token)) {
        return Err(Error::CredentialReflection);
    }
    if page["prefix_validation"] != "prefix-view/0.1-experimental"
        || page["validated_prefix_sha256"] != digest(records)
    {
        return Err(Error::PrefixMismatch);
    }
    let originals: RawPage = serde_json::from_str(raw).map_err(|_| Error::InvalidPage)?;
    records.extend(
        originals
            .events
            .into_iter()
            .map(|event| event.get().to_owned()),
    );
    if page["prefix_sha256"] != digest(records) {
        return Err(Error::PrefixMismatch);
    }
    page["next_cursor"]
        .as_str()
        .map(str::to_owned)
        .ok_or(Error::InvalidPage)
}
