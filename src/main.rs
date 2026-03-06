use anyhow::{Result, anyhow, bail};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_bencode::{self, value::Value as BencodeValue};
use serde_json::{Value as JsonValue, json};
use sha1::{Digest, Sha1};
use std::{
    env,
    fs::{self, File},
    io::Write,
    net::{Ipv4Addr, SocketAddrV4},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
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
            let peer_addr_list = get_peer_list(torrent_file).await?;
            for peer_addr in peer_addr_list {
                println!("{peer_addr}");
            }
        }
        "handshake" => {
            let torrent_file = &args[2];
            let peer_addr = &args[3].parse::<SocketAddrV4>()?;
            let mut stream = perform_handshake(torrent_file, peer_addr).await?;
            let mut handshake_buf = [0u8; 68];
            stream.read_exact(&mut handshake_buf).await?;
            let mut peer_id = [0u8; 20];
            peer_id.copy_from_slice(&handshake_buf[48..68]);
            println!("Peer ID: {}", hex::encode(peer_id));
        }
        "download_piece" => {
            let download_path = &args[3];
            let torrent_file = &args[4];
            let piece_idx = &args[5].parse::<u32>()?;
            handle_piece_download(torrent_file, piece_idx, download_path).await?;
        }
        _ => println!("unknown command: {}", args[1]),
    }
    Ok(())
}

async fn handle_piece_download(
    torrent_file: &str,
    piece_idx: &u32,
    download_path: &str,
) -> Result<()> {
    let decoded_contents = get_torrent_file_info(torrent_file)?;
    let pieces_hash = decoded_contents.info.pieces;
    let num_pieces = pieces_hash.len() / 20;
    if *piece_idx as usize >= num_pieces {
        bail!(
            "piece_idx {} out of range (total: {})",
            piece_idx,
            num_pieces
        );
    }
    let peer_addr_list = &get_peer_list(torrent_file).await?;
    let mut stream = choose_peer(torrent_file, peer_addr_list).await?;

    let mut handshake_response = [0u8; 68];
    stream.read_exact(&mut handshake_response).await?;

    let mut buf = [0u8; 4];
    stream.read_exact(&mut buf).await?;
    let msg_length = u32::from_be_bytes(buf) as usize;

    let mut msg_body = vec![0u8; msg_length];
    stream.read_exact(&mut msg_body).await?;

    let msg_id = msg_body[0];
    if msg_id != 5 {
        // ignore this case for now: "Downloaders which don't have anything yet may skip the 'bitfield' message"
        bail!("Expected bitfield as the first message");
    }
    // ignore payload for now, assuming all peers have all pieces
    let _bitfield_payload = &msg_body[1..];

    let mut interested_msg = [0u8; 5];
    interested_msg[0..4].copy_from_slice(&1u32.to_be_bytes());
    interested_msg[4] = 2u8;
    stream.write_all(&interested_msg).await?;

    let mut buf = [0u8; 5];
    stream.read_exact(&mut buf).await?;
    let msg_id = buf[4];
    if msg_id != 1 {
        // ignore other cases for now
        bail!("Expected an unchoke message")
    }

    let length = decoded_contents.info.length;
    let piece_length = decoded_contents.info.piece_length;
    let piece_length = if *piece_idx as u64 == length / piece_length as u64 {
        (length % piece_length as u64) as u32
    } else {
        piece_length
    };
    let block_size = 2u32.pow(14);
    let num_blocks = piece_length.div_ceil(block_size);
    for idx in 0..num_blocks {
        let mut request_msg = [0u8; 17];
        request_msg[0..4].copy_from_slice(&13u32.to_be_bytes());
        request_msg[4] = 6u8;
        request_msg[5..9].copy_from_slice(&piece_idx.to_be_bytes());
        let begin = idx * block_size;
        request_msg[9..13].copy_from_slice(&begin.to_be_bytes());
        let length = if idx == num_blocks - 1 {
            piece_length - begin
        } else {
            block_size
        };
        request_msg[13..17].copy_from_slice(&length.to_be_bytes());
        stream.write_all(&request_msg).await?;
    }

    let mut piece_buffer = vec![0u8; piece_length as usize];
    let mut bytes_received = 0;
    while bytes_received < piece_length {
        let mut buf = [0u8; 4];
        stream.read_exact(&mut buf).await?;
        let msg_length = u32::from_be_bytes(buf) as usize;
        let mut msg_body = vec![0u8; msg_length];
        stream.read_exact(&mut msg_body).await?;
        let msg_id = msg_body[0];
        if msg_id != 7 {
            // ignore other cases for now
            bail!("Expected an unchoke message")
        }
        let _index = u32::from_be_bytes(msg_body[1..5].try_into()?);
        let begin = u32::from_be_bytes(msg_body[5..9].try_into()?);
        let block_data = &msg_body[9..];
        piece_buffer[begin as usize..begin as usize + block_data.len()].copy_from_slice(block_data);
        bytes_received += block_data.len() as u32;
    }

    let hash_start_idx = *piece_idx as usize * 20;
    let piece_hash = &pieces_hash[hash_start_idx..hash_start_idx + 20];
    if piece_hash != &generate_hash(&piece_buffer) {
        bail!("Received piece does not match piece hash in the torrent file");
    }
    let mut file = File::create(download_path)?;
    file.write_all(&piece_buffer)?;
    Ok(())
}

async fn choose_peer(torrent_file: &str, peer_addr_list: &[SocketAddrV4]) -> Result<TcpStream> {
    if peer_addr_list.is_empty() {
        bail!("Tracker returned 0 peers: cannot initiate handshake");
    }
    for peer_addr in peer_addr_list.iter().take(5) {
        if let Ok(stream) = perform_handshake(torrent_file, peer_addr).await {
            return Ok(stream);
        }
    }

    Err(anyhow!(
        "Failed to handshake with any of the first {} peers",
        peer_addr_list.len().min(5)
    ))
}

async fn perform_handshake(torrent_file: &str, peer_addr: &SocketAddrV4) -> Result<TcpStream> {
    let decoded_contents = get_torrent_file_info(torrent_file)?;
    let bencoded_info = serde_bencode::to_bytes(&decoded_contents.info)?;
    let bencoded_info_hash = generate_hash(&bencoded_info);

    let mut stream = TcpStream::connect(peer_addr).await?;
    let mut handshake_msg = [0u8; 68];
    handshake_msg[0] = 19u8;
    handshake_msg[1..20].copy_from_slice(b"BitTorrent protocol");
    handshake_msg[20..28].copy_from_slice(&[0u8; 8]);
    handshake_msg[28..48].copy_from_slice(&bencoded_info_hash);
    handshake_msg[48..68].copy_from_slice(&generate_peer_id());
    stream.write_all(&handshake_msg).await?;

    Ok(stream)
}

async fn get_peer_list(torrent_file: &str) -> Result<Vec<SocketAddrV4>> {
    let decoded_contents = get_torrent_file_info(torrent_file)?;
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
        TrackerResponse::Success { interval: _, peers } => Ok(peers
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

fn get_torrent_file_info(torrent_file: &str) -> Result<TorrentFile> {
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
