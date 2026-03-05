use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_bencode::{self, value::Value as BencodeValue};
use serde_json::{Value as JsonValue, json};
use sha1::{Digest, Sha1};
use std::{
    env,
    error::Error,
    fs,
    net::{Ipv4Addr, SocketAddrV4},
};
use url::form_urlencoded;

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

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum TrackerResponse {
    Failure {
        #[serde(rename = "failure reason")]
        failure_reason: String,
    },
    Success {
        interval: u64,
        peers: serde_bytes::ByteBuf,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
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
            let decoded_contents = get_torrent_file_info(torrent_file)?;
            let bencoded_info = serde_bencode::to_bytes(&decoded_contents.info)?;
            let bencoded_info_hash = generate_hash_hex(&bencoded_info);
            println!("Tracker URL: {}", decoded_contents.announce);
            println!("Length: {}", decoded_contents.info.length);
            println!("Info Hash: {}", bencoded_info_hash);
            println!("Piece Length: {}", decoded_contents.info.piece_length);
            println!("Piece Hashes:");
            for chunk in decoded_contents.info.pieces.chunks_exact(20) {
                println!("{}", hex::encode(chunk))
            }
        }
        "peers" => {
            let torrent_file = &args[2];
            let decoded_contents = get_torrent_file_info(torrent_file)?;
            let bencoded_info = serde_bencode::to_bytes(&decoded_contents.info)?;
            let bencoded_info_hash = generate_hash(&bencoded_info);
            let encoded_hash: String =
                form_urlencoded::byte_serialize(&bencoded_info_hash).collect();
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
                TrackerResponse::Success { interval: _, peers } => {
                    let socket_addresses: Vec<SocketAddrV4> = peers
                        .chunks_exact(6)
                        .map(|chunk| {
                            let ip = Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3]);
                            let port = u16::from_be_bytes([chunk[4], chunk[5]]);
                            SocketAddrV4::new(ip, port)
                        })
                        .collect();
                    for addr in socket_addresses {
                        println!("{}", addr.to_string());
                    }
                }
                TrackerResponse::Failure { failure_reason } => {
                    eprintln!("Tracker error: {failure_reason}");
                }
            }
        }
        _ => println!("unknown command: {}", args[1]),
    }
    Ok(())
}

fn generate_peer_id() -> [u8; 20] {
    let mut random_bytes = [0u8; 20];
    rand::rng().fill_bytes(&mut random_bytes);
    random_bytes
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

fn get_torrent_file_info(torrent_file: &String) -> Result<TorrentFile, Box<dyn Error>> {
    let bencoded_contents = fs::read(torrent_file)?;
    let decoded_contents = serde_bencode::from_bytes(&bencoded_contents)?;
    Ok(decoded_contents)
}

fn generate_hash(bencoded_info: &[u8]) -> [u8; 20] {
    Sha1::digest(bencoded_info).into()
}

fn generate_hash_hex(bencoded_info: &[u8]) -> String {
    let hash = Sha1::digest(bencoded_info);
    hex::encode(hash)
}
