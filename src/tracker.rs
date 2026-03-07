use crate::{
    torrent::get_torrent_file_info,
    utils::{generate_hash, generate_peer_id},
};
use anyhow::Result;
use serde::Deserialize;
use std::net::{Ipv4Addr, SocketAddrV4};
use url::form_urlencoded;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum TrackerResponse {
    Failure {
        #[serde(rename = "failure reason")]
        failure_reason: String,
    },
    Success {
        _interval: u64,
        peers: serde_bytes::ByteBuf,
    },
}

pub(crate) async fn get_peer_list(torrent_file: &str) -> Result<Vec<SocketAddrV4>> {
    let decoded_contents = get_torrent_file_info(torrent_file).await?;
    let bencoded_info = serde_bencode::to_bytes(&decoded_contents.info)?;
    let bencoded_info_hash = generate_hash(&bencoded_info);
    let encoded_hash: String = form_urlencoded::byte_serialize(&bencoded_info_hash).collect();
    let peer_id: String = form_urlencoded::byte_serialize(&generate_peer_id()).collect();

    let query_params = [
        ("port", 6881.to_string()),
        ("uploaded", 0.to_string()),
        ("downloaded", 0.to_string()),
        ("left", decoded_contents.info.length.to_string()),
        ("compact", 1.to_string()),
    ];

    let client = reqwest::Client::new();
    let tracker_url = format!(
        "{}?info_hash={encoded_hash}&peer_id={peer_id}",
        decoded_contents.announce
    );
    let response_bytes = client
        .get(tracker_url)
        .query(&query_params)
        .send()
        .await?
        .bytes()
        .await?;
    let tracker_data: TrackerResponse = serde_bencode::from_bytes(&response_bytes)?;

    match tracker_data {
        TrackerResponse::Success { _interval, peers } => Ok(peers
            .chunks_exact(6)
            .map(|chunk| {
                let ip = Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3]);
                let port = u16::from_be_bytes([chunk[4], chunk[5]]);
                SocketAddrV4::new(ip, port)
            })
            .collect()),
        TrackerResponse::Failure { failure_reason } => {
            eprintln!("Tracker error: {failure_reason}");
            Ok(Vec::new())
        }
    }
}
