const HEADER_PREFIX: &str = "omhl-byproduct v1 stdout=";
const HEADER_STDERR: &str = " stderr=";

pub fn encode(stdout: &[u8], stderr: &[u8]) -> Vec<u8> {
    let mut blob = format!(
        "{HEADER_PREFIX}{}{HEADER_STDERR}{}\n",
        stdout.len(),
        stderr.len()
    )
    .into_bytes();
    blob.extend_from_slice(stdout);
    blob.extend_from_slice(stderr);

    blob
}

pub fn decode(blob: &[u8]) -> Option<(&[u8], &[u8])> {
    let newline = blob.iter().position(|byte| *byte == b'\n')?;
    let header = std::str::from_utf8(&blob[..newline]).ok()?;
    let (stdout_size, stderr_size) = header
        .strip_prefix(HEADER_PREFIX)?
        .split_once(HEADER_STDERR)
        .and_then(|(out, err)| Some((out.parse::<usize>().ok()?, err.parse::<usize>().ok()?)))?;
    let body = &blob[newline + 1..];
    if body.len() != stdout_size.checked_add(stderr_size)? {
        return None;
    }

    Some(body.split_at(stdout_size))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByproductHash(String);

impl ByproductHash {
    pub fn parse(text: &str) -> Result<ByproductHash, ByproductError> {
        if hexlog::Hash::parse(text).is_ok() {
            return Ok(ByproductHash(text.to_owned()));
        }
        Err(ByproductError {
            kind: ByproductErrorKind::InvalidHash,
            message: format!("hash is not 64 lowercase hex characters: {text}"),
        })
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByproductErrorKind {
    Io,
    InvalidHash,
    Corrupt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByproductError {
    pub kind: ByproductErrorKind,
    pub message: String,
}

pub trait ByproductStore: Send + Sync {
    fn put(&self, bytes: &[u8]) -> Result<ByproductHash, ByproductError>;
    fn get(&self, hash: &ByproductHash) -> Result<Option<Vec<u8>>, ByproductError>;
}

#[cfg(test)]
mod tests;
