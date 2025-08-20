use anyhow::Context;
use clap::{Parser, Subcommand};
use serde::{Deserialize, de::Visitor};
use serde_json;
use std::path::PathBuf;

#[derive(Parser, Debug)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Decode { value: String },
    Info { torrent: PathBuf },
}

fn decode_bencoded_value(encoded_value: &str) -> anyhow::Result<(serde_json::Value, &str)> {
    match encoded_value.bytes().next() {
        Some(b'i') => {
            if let Some((n, rest)) = encoded_value
                .strip_prefix('i')
                .and_then(|rest| (&*rest).split_once('e'))
            {
                let n = n.parse::<i64>()?;
                return Ok((n.into(), rest));
            }
        }
        Some(b'l') => {
            let mut items = vec![];
            let mut rest = encoded_value.split_at(1).1;
            while !rest.starts_with('e') {
                let (v, reminder) = decode_bencoded_value(rest)?;
                items.push(v);
                rest = reminder;
            }
            return Ok((items.into(), &rest[1..]));
        }
        Some(b'd') => {
            let mut items = serde_json::Map::new();
            let mut rest = encoded_value.split_at(1).1;
            while !rest.starts_with('e') {
                let (k, reminder) = decode_bencoded_value(rest)?;
                let k = match k {
                    serde_json::Value::String(k) => k,
                    _ => anyhow::bail!("Dictionary keys must be strings, found: {}", k),
                };
                let (v, reminder) = decode_bencoded_value(reminder)?;
                items.insert(k, v);
                rest = reminder;
            }
            return Ok((items.into(), &rest[1..]));
        }
        Some(b'0'..=b'9') => {
            if let Some((len, rest)) = encoded_value.split_once(':') {
                if let Ok(len) = len.parse::<usize>() {
                    return Ok((rest[..len].to_string().into(), &rest[len..]));
                }
            }
        }
        _ => {}
    }
    anyhow::bail!("Invalid bencoded value: {}", encoded_value)
}

#[derive(Debug, Clone, Deserialize)]
struct Torrent {
    announce: String, //reqwest::Url,
    info: Info,
}

#[derive(Debug, Clone, Deserialize)]
struct Info {
    name: String,
    #[serde(rename = "piece length")]
    piece_length: usize,
    pieces: Hashes,
    #[serde(flatten)]
    keys: Keys,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum Keys {
    SingleFile { length: usize },
    MultiFile { file: Vec<File> },
}

#[derive(Debug, Clone, Deserialize)]
struct File {
    length: usize,
    path: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Decode { value } => {
            // let v: serde_json::Value =
            //     serde_bencode::from_str(&value).context("decode bencoded value")?;

            let v = decode_bencoded_value(&value)?.0.to_string();

            println!("{v}");
        }
        Commands::Info { torrent } => {
            // Handle the Info command
            let torrent = std::fs::read(torrent).context("read torrent file")?;
            let t: Torrent =
                serde_bencode::from_bytes(&torrent).context("deserialize torrent file")?;

            println!("Tracker URL: {}", t.announce);
            if let Keys::SingleFile { length } = t.info.keys {
                println!("Length: {length}");
            }
        }
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct Hashes(Vec<[u8; 20]>);
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

impl<'de> Deserialize<'de> for Hashes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_bytes(HashesVisitor)
    }
}
