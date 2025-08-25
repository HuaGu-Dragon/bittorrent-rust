use anyhow::{Context, Result};

use crate::{
    torrent::{File, Torrent},
    tracker::TrackerResponse,
};

pub(crate) async fn download_all(t: &Torrent) -> Result<Downloaded> {
    let peer_info = TrackerResponse::query(t)
        .await
        .context("query tracker for peer info")?;
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
