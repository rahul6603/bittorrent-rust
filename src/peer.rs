use crate::{
    torrent::get_torrent_file_info,
    utils::{generate_hash, generate_peer_id},
};
use anyhow::{Result, anyhow, bail};
use std::net::SocketAddrV4;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

pub(crate) async fn choose_peer(
    torrent_file: &str,
    peer_addr_list: &[SocketAddrV4],
) -> Result<TcpStream> {
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

pub(crate) async fn perform_handshake(
    torrent_file: &str,
    peer_addr: &SocketAddrV4,
) -> Result<TcpStream> {
    let decoded_contents = get_torrent_file_info(torrent_file).await?;
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

    let mut handshake_buf = [0u8; 68];
    stream.read_exact(&mut handshake_buf).await?;

    Ok(stream)
}
