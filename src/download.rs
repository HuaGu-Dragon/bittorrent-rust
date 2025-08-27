use std::collections::BinaryHeap;

use anyhow::{Context, Result};
use futures_util::StreamExt;

use crate::{
    BLOCK_MAX_SIZE,
    peer::{Peer, Request},
    piece::Piece,
    torrent::{File, Torrent},
    tracker::TrackerResponse,
};

pub(crate) async fn download_all(t: &Torrent) -> Result<Downloaded> {
    let info_hash = t.info_hash();
    let peer_info = TrackerResponse::query(t, info_hash)
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

    while let Some(piece) = need_pieces.pop() {
        let blocks_num = (piece.length() as u32 + BLOCK_MAX_SIZE - 1) / BLOCK_MAX_SIZE;
        // let mut all_blocks = Vec::with_capacity(piece.length());
        let peers = peers
            .iter_mut()
            .enumerate()
            .filter_map(|(peer_i, peer)| piece.peers().contains(&peer_i).then_some(peer));

        for block in 0..blocks_num {
            let block_size = if block == blocks_num - 1 {
                let md = piece.length() as u32 % BLOCK_MAX_SIZE;
                if md == 0 { BLOCK_MAX_SIZE } else { md }
            } else {
                BLOCK_MAX_SIZE
            };
            let mut request = Request::new(
                piece.piece_i as u32,
                (block * BLOCK_MAX_SIZE) as u32,
                block_size as u32,
            );
            let request_bytes = Vec::from(request.as_bytes_mut());
            // peer.send(Message {
            //     tag: MessageTag::Request,
            //     payload: request_bytes,
            // })
            // .await
            // .with_context(|| format!("send request for block {block}"))?;

            // let piece = peer.next().await.context("read piece message")??;
            // assert_eq!(piece.tag, MessageTag::Piece);
            // let piece =
            //     Piece::ref_from_bytes(&piece.payload[..]).context("deserialize piece message")?;
            // assert_eq!(piece.begin() as usize, block * BLOCK_MAX_SIZE);
            // assert_eq!(piece.block().len(), block_size);

            // all_blocks.extend(piece.block());
        }
    }

    todo!()
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
