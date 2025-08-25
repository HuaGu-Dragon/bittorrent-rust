use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize, de::Visitor};
use sha1::{Digest, Sha1};

#[derive(Debug, Clone, Deserialize)]
pub struct Torrent {
    pub announce: String, //reqwest::Url,
    pub info: Info,
}

impl Torrent {
    pub fn info_hash(&self) -> [u8; 20] {
        let info_hash = serde_bencode::to_bytes(&self.info).expect("serialize info");
        let mut hasher = Sha1::new();
        hasher.update(info_hash);
        hasher.finalize().into()
    }

    pub async fn read(file: impl AsRef<Path>) -> Result<Self> {
        let torrent = tokio::fs::read(file).await.context("read torrent file")?;
        let t: Torrent = serde_bencode::from_bytes(&torrent).context("deserialize torrent file")?;

        Ok(t)
    }

    pub fn print_tree(&self) {
        match self.info.keys {
            Keys::SingleFile { .. } => {
                println!("{}", self.info.name);
            }
            Keys::MultiFile { ref files } => {
                for file in files {
                    println!("{}", file.path.join(std::path::MAIN_SEPARATOR_STR))
                }
            }
        }
    }

    pub async fn download_all(&self) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Info {
    pub name: String,
    #[serde(rename = "piece length")]
    pub piece_length: usize,
    pub pieces: Hashes,
    #[serde(flatten)]
    pub keys: Keys,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Keys {
    SingleFile { length: usize },
    MultiFile { files: Vec<File> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct File {
    pub length: usize,
    pub path: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Hashes(pub Vec<[u8; 20]>);
struct HashesVisitor;

impl<'de> Visitor<'de> for HashesVisitor {
    type Value = Hashes;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a byte string whose length is multiple of 20")
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if v.len() % 20 != 0 {
            Err(E::invalid_length(v.len(), &self))
        } else {
            Ok(Hashes(
                v.chunks_exact(20)
                    .map(|chunk| chunk.try_into().unwrap())
                    .collect(),
            ))
        }
    }
}

impl Serialize for Hashes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let bytes = self.0.concat();
        serializer.serialize_bytes(&bytes)
    }
}

impl<'de> Deserialize<'de> for Hashes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_bytes(HashesVisitor)
    }
}
