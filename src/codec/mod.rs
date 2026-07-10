use crate::error::{ChronosError, ChronosResult};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum DigestDomain {
    Library,
    Account,
    Asset,
    Pool,
    Position,
    Lock,
    Settlement,
    Expiry,
    Scenario,
}

impl Display for DigestDomain {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Library => "library",
            Self::Account => "account",
            Self::Asset => "asset",
            Self::Pool => "pool",
            Self::Position => "position",
            Self::Lock => "lock",
            Self::Settlement => "settlement",
            Self::Expiry => "expiry",
            Self::Scenario => "scenario",
        };
        f.write_str(label)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct CanonicalDigest(pub [u8; 32]);

impl CanonicalDigest {
    pub fn hex(self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(input: &str) -> ChronosResult<Self> {
        let bytes = hex::decode(input).map_err(|error| ChronosError::Codec(error.to_string()))?;
        if bytes.len() != 32 {
            return Err(ChronosError::Codec("digest must be 32 bytes".to_string()));
        }
        let mut fixed = [0u8; 32];
        fixed.copy_from_slice(&bytes);
        Ok(Self(fixed))
    }
}

impl Display for CanonicalDigest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.hex())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CanonicalEnvelope {
    pub domain: DigestDomain,
    pub subject: String,
    pub fields: Vec<(String, String)>,
}

impl CanonicalEnvelope {
    pub fn new<K, V, I>(domain: DigestDomain, subject: impl Into<String>, fields: I) -> Self
    where
        K: Into<String>,
        V: Into<String>,
        I: IntoIterator<Item = (K, V)>,
    {
        let mut fields = fields
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect::<Vec<_>>();
        fields.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
        Self {
            domain,
            subject: subject.into(),
            fields,
        }
    }

    pub fn push(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.push((key.into(), value.into()));
        self.fields
            .sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
        self
    }

    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(self.domain.to_string().as_bytes());
        out.push(b'\n');
        out.extend_from_slice(self.subject.as_bytes());
        out.push(b'\n');
        for (key, value) in &self.fields {
            out.extend_from_slice(key.as_bytes());
            out.push(b'=');
            out.extend_from_slice(value.as_bytes());
            out.push(b'\n');
        }
        out
    }

    pub fn digest(&self) -> CanonicalDigest {
        CanonicalDigest(*blake3::hash(&self.canonical_bytes()).as_bytes())
    }

    pub fn to_json(&self) -> ChronosResult<String> {
        serde_json::to_string(self).map_err(|error| ChronosError::Codec(error.to_string()))
    }

    pub fn from_json(input: &str) -> ChronosResult<Self> {
        serde_json::from_str(input).map_err(|error| ChronosError::Codec(error.to_string()))
    }
}
