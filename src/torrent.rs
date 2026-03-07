use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::fs;

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct TorrentFile {
    pub(crate) announce: String,
    pub(crate) info: Info,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Info {
    pub(crate) length: u64,
    pub(crate) name: String,
    #[serde(rename = "piece length")]
    pub(crate) piece_length: u32,
    #[serde(with = "serde_bytes")]
    pub(crate) pieces: Vec<u8>,
}

pub(crate) async fn get_torrent_file_info(torrent_file: &str) -> Result<TorrentFile> {
    let bencoded_contents = fs::read(torrent_file).await?;
    let decoded_contents = serde_bencode::from_bytes(&bencoded_contents)?;
    Ok(decoded_contents)
}
