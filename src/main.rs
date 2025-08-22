use anyhow::Context;
use bittorrent_rust::{
    torrent::*,
    tracker::{TrackerRequest, TrackerResponse, url_encode},
};
use clap::{Parser, Subcommand};
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
    Peers { torrent: PathBuf },
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
            let length = if let Keys::SingleFile { length } = t.info.keys {
                length
            } else {
                todo!()
            };

            println!("Length: {length}");

            let info_hash = t.info_hash();

            println!("Info Hash: {}", hex::encode(info_hash));

            println!("Piece Length: {}", t.info.piece_length);

            println!("Piece Hashes:");
            for hash in t.info.pieces.0 {
                println!("{}", hex::encode(hash));
            }
        }
        Commands::Peers { torrent } => {
            let dot_torrent = std::fs::read(torrent).context("read torrent file")?;
            let t: Torrent =
                serde_bencode::from_bytes(&dot_torrent).context("deserialize torrent file")?;

            let length = if let Keys::SingleFile { length } = t.info.keys {
                length
            } else {
                todo!()
            };

            let info_hash = t.info_hash();

            let request = TrackerRequest {
                peer_id: String::from("00112233445566778899"),
                port: 6881,
                uploaded: 0,
                downloaded: 0,
                left: length,
                compact: 1,
            };

            let mut tracker_url =
                reqwest::Url::parse(&t.announce).context("parse tracker announce URL")?;
            let url_params =
                serde_urlencoded::to_string(request).context("serialize tracker request")?;

            let url_params = format!("info_hash={}&{}", &url_encode(&info_hash), url_params);
            tracker_url.set_query(Some(&url_params));

            let response = reqwest::get(tracker_url)
                .await
                .context("send tracker request")?;
            let response = response.bytes().await.context("read tracker response")?;
            let response: TrackerResponse =
                serde_bencode::from_bytes(&response).context("deserialize tracker response")?;

            for peer in response.peers.0 {
                println!("{} {}", peer.ip(), peer.port());
            }
        }
    }

    Ok(())
}
