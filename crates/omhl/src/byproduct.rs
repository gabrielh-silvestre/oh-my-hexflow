/// 64 hexadecimais minúsculos.
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
    /// Confere que o sha256 dos bytes é o próprio hash.
    fn get(&self, hash: &ByproductHash) -> Result<Option<Vec<u8>>, ByproductError>;
}
