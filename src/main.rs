use crate::{
    torrent::get_torrent_file_info,
    tracker::get_peer_list,
    utils::{bencode_to_json, generate_hash},
};
use anyhow::Result;
use serde_bencode::{self, value::Value as BencodeValue};
use std::env;

mod download;
mod peer;
mod torrent;
mod tracker;
mod utils;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];

    match command.as_str() {
        "decode" => {
            let bencoded_contents = &args[2];
            let decoded_contents: BencodeValue = serde_bencode::from_str(bencoded_contents)?;
            let decoded_json = serde_json::to_string_pretty(&bencode_to_json(decoded_contents))?;
            println!("{decoded_json}");
        }
        "info" => {
            let torrent_file = &args[2];
            let decoded_contents = get_torrent_file_info(torrent_file).await?;
            let bencoded_info = serde_bencode::to_bytes(&decoded_contents.info)?;
            let bencoded_info_hash_hex = hex::encode(generate_hash(&bencoded_info));

            println!("Tracker URL: {}", decoded_contents.announce);
            println!("Length: {}", decoded_contents.info.length);
            println!("Info Hash: {}", bencoded_info_hash_hex);
            println!("Piece Length: {}", decoded_contents.info.piece_length);
            println!("Piece Hashes:");

            for chunk in decoded_contents.info.pieces.chunks_exact(20) {
                println!("{}", hex::encode(chunk))
            }
        }
        "peers" => {
            let torrent_file = &args[2];
            let peer_addr_list = get_peer_list(torrent_file).await?;
            for peer_addr in peer_addr_list {
                println!("{peer_addr}");
            }
        }
        "download" => {
            let download_path = &args[3];
            let torrent_file = &args[4];
            download::handle_file_download(torrent_file, download_path).await?;
        }
        _ => println!("unknown command: {}", args[1]),
    }
    Ok(())
}
