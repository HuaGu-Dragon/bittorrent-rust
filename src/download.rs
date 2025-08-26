use std::collections::BinaryHeap;

use anyhow::{Context, Result};
use futures_util::StreamExt;

use crate::{
    peer::Peer,
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

    let peers = peer_list;

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
