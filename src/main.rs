use serde::{Deserialize, Serialize};
use serde_bencode;
use sha1::{Digest, Sha1};
use std::{env, error::Error, fs};

#[derive(Serialize, Deserialize, Debug)]
struct TorrentFile {
    announce: String,
    info: Info,
}

#[derive(Serialize, Deserialize, Debug)]
struct Info {
    length: u64,
    name: String,
    #[serde(rename = "piece length")]
    piece_length: u32,
    #[serde(with = "serde_bytes")]
    pieces: Vec<u8>,
}

fn decode_bencoded_value(bencoded_value: &[u8]) -> Result<TorrentFile, serde_bencode::Error> {
    serde_bencode::from_bytes(bencoded_value)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];

    match command.as_str() {
        "info" => {
            let torrent_file = &args[2];
            get_torrent_file_info(torrent_file)?;
        }
        _ => println!("unknown command: {}", args[1]),
    }
    Ok(())
}

fn get_torrent_file_info(torrent_file: &String) -> Result<(), Box<dyn Error>> {
    let bencoded_contents = fs::read(torrent_file)?;
    let decoded_contents = decode_bencoded_value(&bencoded_contents)?;
    let bencoded_info = serde_bencode::to_bytes(&decoded_contents.info)?;
    let bencoded_info_hash = generate_sha1_hash(&bencoded_info);

    println!("Tracker URL: {}", decoded_contents.announce);
    println!("Length: {}", decoded_contents.info.length);
    println!("Info Hash: {}", bencoded_info_hash);
    println!("Piece Length: {}", decoded_contents.info.piece_length);
    println!("Piece Hashes:");
    for chunk in decoded_contents.info.pieces.chunks(20) {
        println!("{}", hex::encode(chunk))
    }
    Ok(())
}

fn generate_sha1_hash(bencoded_info: &[u8]) -> String {
    let hash = Sha1::digest(bencoded_info);
    hex::encode(hash)
}
