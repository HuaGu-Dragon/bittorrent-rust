use anyhow::Context;
use bittorrent_rust::{
    peer::{Handshake, Message, MessageFramer, MessageTag, Piece, Request},
    torrent::*,
    tracker::*,
};
use clap::{Parser, Subcommand};
use futures_util::{SinkExt, StreamExt};
use serde_json;
use sha1::{Digest, Sha1};
use std::{net::SocketAddrV4, path::PathBuf, str::FromStr};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const BLOCK_MAX_SIZE: usize = 1 << 14;

#[derive(Parser, Debug)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
#[clap(rename_all = "snake_case")]
enum Commands {
    Decode {
        value: String,
    },
    Info {
        torrent: PathBuf,
    },
    Peers {
        torrent: PathBuf,
    },
    Handshake {
        torrent: PathBuf,
        peer: String,
    },
    DownloadPiece {
        #[arg(short)]
        output: PathBuf,
        torrent: PathBuf,
        piece: usize,
    },
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

            let v = decode_bencoded_value(&value)
                .context("decode bencoded value")?
                .0
                .to_string();

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
        Commands::Handshake { torrent, peer } => {
            let dot_torrent = std::fs::read(torrent).context("read torrent file")?;
            let t: Torrent =
                serde_bencode::from_bytes(&dot_torrent).context("deserialize torrent file")?;

            let info_hash = t.info_hash();

            let peer = SocketAddrV4::from_str(peer.as_str()).context("parse peer address")?;

            let mut peer = tokio::net::TcpStream::connect(peer)
                .await
                .context("connect to peer")?;

            let mut handshake = Handshake::new(info_hash, *b"00112233445566778899");
            {
                let handshake_bytes = handshake.as_bytes_mut();

                peer.write_all(handshake_bytes)
                    .await
                    .context("write handshake")?;

                peer.read_exact(handshake_bytes)
                    .await
                    .context("read handshake")?;
            }
            println!("Peer ID: {}", hex::encode(handshake.peer_id));
        }
        Commands::DownloadPiece {
            output,
            torrent,
            piece,
        } => {
            let dot_torrent = std::fs::read(torrent).context("read torrent file")?;
            let t: Torrent =
                serde_bencode::from_bytes(&dot_torrent).context("deserialize torrent file")?;
            assert!(piece < t.info.pieces.0.len(), "Piece index out of bounds");

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

            let peer = response.peers.0.first().context("no peers found")?;

            let mut peer = tokio::net::TcpStream::connect(peer)
                .await
                .context("connect to peer")?;

            let mut handshake = Handshake::new(info_hash, *b"00112233445566778899");
            {
                let handshake_bytes = handshake.as_bytes_mut();

                peer.write_all(handshake_bytes)
                    .await
                    .context("write handshake")?;

                peer.read_exact(handshake_bytes)
                    .await
                    .context("read handshake")?;
            }

            let mut peer = tokio_util::codec::Framed::new(peer, MessageFramer);

            let bit_field = peer
                .next()
                .await
                .context("read message expected BitField")??;
            assert_eq!(bit_field.tag, MessageTag::BitField);

            peer.send(Message {
                tag: MessageTag::Interested,
                payload: Vec::new(),
            })
            .await
            .context("send message with request")?;

            let un_choke = peer
                .next()
                .await
                .context("read message expected UnChoke")??;
            assert_eq!(un_choke.tag, MessageTag::UnChoke);
            assert!(un_choke.payload.is_empty());

            let piece_hash = t.info.pieces.0[piece];
            let piece_size = if piece == t.info.pieces.0.len() - 1 {
                let md = length % t.info.piece_length;
                if md == 0 { t.info.piece_length } else { md }
            } else {
                t.info.piece_length
            };

            let blocks_num = (piece_size + BLOCK_MAX_SIZE - 1) / BLOCK_MAX_SIZE;
            let mut all_blocks = Vec::with_capacity(piece_size);
            for block in 0..blocks_num {
                let block_size = if block == blocks_num - 1 {
                    let md = piece_size % BLOCK_MAX_SIZE;
                    if md == 0 { BLOCK_MAX_SIZE } else { md }
                } else {
                    BLOCK_MAX_SIZE
                };
                let mut request = Request::new(
                    piece as u32,
                    (block * BLOCK_MAX_SIZE) as u32,
                    block_size as u32,
                );
                let request_bytes = Vec::from(request.as_bytes_mut());
                peer.send(Message {
                    tag: MessageTag::Request,
                    payload: request_bytes,
                })
                .await
                .with_context(|| format!("send request for block {block}"))?;

                let piece = peer.next().await.context("read piece message")??;
                assert_eq!(piece.tag, MessageTag::Piece);
                let piece = Piece::ref_from_bytes(&piece.payload[..])
                    .context("deserialize piece message")?;
                assert_eq!(piece.begin() as usize, block * BLOCK_MAX_SIZE);
                assert_eq!(piece.block().len(), block_size);

                all_blocks.extend(piece.block());
            }
            assert_eq!(all_blocks.len(), piece_size);

            let mut hasher = Sha1::new();
            hasher.update(&all_blocks);
            let hash: [u8; 20] = hasher.finalize().into();
            assert_eq!(hash, piece_hash, "Piece hash mismatch");

            tokio::fs::write(&output, all_blocks)
                .await
                .context("write piece to output file")?;
            println!("Piece {piece} downloaded to {}", output.display())
        }
    }

    Ok(())
}
