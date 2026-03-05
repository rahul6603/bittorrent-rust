use serde::{Deserialize, Serialize};
use serde_bencode::{self, value::Value as BencodeValue};
use serde_json::{Value as JsonValue, json};
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
        "decode" => {
            let bencoded_contents = &args[2];
            let decoded_contents: BencodeValue = serde_bencode::from_str(bencoded_contents)?;
            let decoded_json = serde_json::to_string_pretty(&bencode_to_json(decoded_contents))?;
            println!("{decoded_json}");
        }
        "info" => {
            let torrent_file = &args[2];
            get_torrent_file_info(torrent_file)?;
        }
        _ => println!("unknown command: {}", args[1]),
    }
    Ok(())
}

fn bencode_to_json(bencoded_contents: BencodeValue) -> JsonValue {
    match bencoded_contents {
        BencodeValue::Bytes(bytes) => match String::from_utf8(bytes) {
            Ok(str) => json!(str),
            Err(e) => json!(hex::encode(e.into_bytes())),
        },
        BencodeValue::Int(int) => json!(int),
        BencodeValue::List(list) => {
            JsonValue::Array(list.into_iter().map(bencode_to_json).collect())
        }
        BencodeValue::Dict(d) => {
            let mut map = serde_json::Map::new();
            for (k, v) in d {
                let key = String::from_utf8_lossy(&k).into_owned();
                map.insert(key, bencode_to_json(v));
            }
            JsonValue::Object(map)
        }
    }
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
