use std::collections::BinaryHeap;

use anyhow::{Context, Result};
use futures_util::StreamExt;
use sha1::{Digest, Sha1};

use crate::{
    BLOCK_MAX_SIZE,
    peer::Peer,
    piece::Piece,
    torrent::{File, Torrent},
    tracker::TrackerResponse,
};

pub(crate) async fn download_all(t: Torrent) -> Result<Downloaded> {
    let info_hash = t.info_hash();
    let peer_info = TrackerResponse::query(&t, info_hash)
        .await
        .context("query tracker for peer info")?;

    let mut peer_list = Vec::new();
    let mut peers = futures_util::stream::iter(peer_info.peers.0.iter())
        .map(|&peer_addr| async move {
            let peer = Peer::new(peer_addr, info_hash).await;
            (peer_addr, peer)
        })
        .buffer_unordered(5);
    while let Some((peer_addr, peer)) = peers.next().await {
        match peer {
            Ok(peer) => peer_list.push(peer),
            Err(e) => eprint!("failed to connect to peer {peer_addr:?}: {e:?}"),
        }
    }
    drop(peers);

    let mut peers = peer_list;

    let mut need_pieces = BinaryHeap::new();
    let mut no_peers = Vec::new();

    for piece_i in 0..t.info.pieces.0.len() {
        let piece = Piece::new(piece_i, &t, &peers);
        if piece.peers().is_empty() {
            no_peers.push(piece);
        } else {
            need_pieces.push(piece);
        }
    }

    assert!(no_peers.is_empty(), "pieces with no peers: {no_peers:?}");

    let mut all_pieces = vec![0u8; t.length()];
    while let Some(piece) = need_pieces.pop() {
        let blocks_num = (piece.length() as u32 + BLOCK_MAX_SIZE - 1) / BLOCK_MAX_SIZE;

        let peers: Vec<_> = peers
            .iter_mut()
            .enumerate()
            .filter_map(|(peer_i, peer)| piece.peers().contains(&peer_i).then_some(peer))
            .collect();

        let (submit, tasks) = kanal::bounded_async(blocks_num as usize);
        for block in 0..blocks_num {
            submit.send(block).await.expect("send block index to tasks");
        }
        let (finish, mut done) = tokio::sync::mpsc::channel(blocks_num as usize);
        let mut participates = futures_util::stream::futures_unordered::FuturesUnordered::new();
        for peer in peers {
            participates.push(peer.participate(
                piece.index(),
                piece.length(),
                blocks_num,
                submit.clone(),
                tasks.clone(),
                finish.clone(),
            ));
        }
        drop(submit);
        drop(finish);
        drop(tasks);

        let mut all_blocks = vec![0u8; piece.length() as usize];
        let mut bytes_received = 0;
        loop {
            tokio::select! {
                joined = participates.next() , if !participates.is_empty() => {
                    match joined {
                        None => {},
                        Some(Ok(_)) => {},
                        Some(Err(e)) => eprintln!("peer task failed: {e:?}"),
                    }
                },
                message = done.recv() => {
                    if let Some(message) = message {
                        let piece = crate::peer::Piece::ref_from_bytes(&message.payload[..])
                            .context("deserialize piece message")?;
                        all_blocks[piece.begin() as usize..].copy_from_slice(piece.block());
                        bytes_received += piece.block().len();
                    } else {
                        break;
                    }
                }
            }
        }
        drop(participates);

        if bytes_received == piece.length() as usize {
            // All blocks received
        } else {
            // Some blocks are missing, re-add the piece to the heap
            anyhow::bail!("some blocks are missing for piece {}", piece.index());
        }

        let mut hasher = Sha1::new();
        hasher.update(&all_blocks);
        let result: [u8; 20] = hasher.finalize().into();
        assert_eq!(&result, piece.hash());

        all_pieces[piece.index() as usize * t.info.piece_length..].copy_from_slice(&all_blocks);
    }

    Ok(Downloaded {
        bytes: all_pieces,
        files: match t.info.keys {
            crate::torrent::Keys::SingleFile { length } => vec![File {
                length,
                path: vec![t.info.name],
            }],
            crate::torrent::Keys::MultiFile { files } => files,
        },
    })
}

pub struct Downloaded {
    bytes: Vec<u8>,
    files: Vec<File>,
}

impl<'a> IntoIterator for &'a Downloaded {
    type Item = DownloadFile<'a>;

    type IntoIter = DownloadedIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        DownloadedIter::new(self)
    }
}

pub struct DownloadedIter<'a> {
    downloaded: &'a Downloaded,
    file_iter: std::slice::Iter<'a, File>,
    offset: usize,
}

impl<'a> DownloadedIter<'a> {
    pub fn new(downloaded: &'a Downloaded) -> Self {
        Self {
            downloaded,
            file_iter: downloaded.files.iter(),
            offset: 0,
        }
    }
}

impl<'a> Iterator for DownloadedIter<'a> {
    type Item = DownloadFile<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let file = self.file_iter.next()?;
        let bytes = &self.downloaded.bytes[self.offset..][..file.length];
        Some(DownloadFile { file, bytes })
    }
}

pub struct DownloadFile<'a> {
    file: &'a File,
    bytes: &'a [u8],
}

impl<'a> DownloadFile<'a> {
    pub fn path(&self) -> &'a [String] {
        &self.file.path
    }

    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }
}
