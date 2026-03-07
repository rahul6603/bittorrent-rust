use crate::{
    peer::choose_peer,
    torrent::{TorrentFile, get_torrent_file_info},
    tracker::get_peer_list,
    utils::generate_hash,
};
use anyhow::{Result, bail};
use std::io::SeekFrom::Start;
use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    net::TcpStream,
};

pub(crate) async fn handle_file_download(torrent_file: &str, download_path: &str) -> Result<()> {
    let decoded_contents = get_torrent_file_info(torrent_file).await?;
    let peer_addr_list = &get_peer_list(torrent_file).await?;

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(download_path)
        .await?;
    file.set_len(decoded_contents.info.length).await?;

    let mut stream = choose_peer(torrent_file, peer_addr_list).await?;
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

    let pieces_hash = &decoded_contents.info.pieces;
    if pieces_hash.len() % 20 != 0 {
        bail!("Invalid pieces field in torrent file: length not a multiple of 20");
    }
    let num_pieces = pieces_hash.len() / 20;

    for idx in 0..num_pieces {
        handle_piece_download(
            &decoded_contents,
            &mut stream,
            &mut file,
            idx as u32,
            num_pieces as u32,
        )
        .await?;
    }
    Ok(())
}

pub(crate) async fn handle_piece_download(
    decoded_contents: &TorrentFile,
    stream: &mut TcpStream,
    file: &mut File,
    piece_idx: u32,
    num_pieces: u32,
) -> Result<()> {
    let pieces_hash = &decoded_contents.info.pieces;
    if piece_idx >= num_pieces {
        bail!(
            "piece_idx {} out of range (total: {})",
            piece_idx,
            num_pieces
        );
    }

    let length = decoded_contents.info.length;
    let piece_length = decoded_contents.info.piece_length;
    let actual_piece_length = if piece_idx == num_pieces as u32 - 1 {
        (length % piece_length as u64) as u32
    } else {
        piece_length
    };
    let block_size = 2u32.pow(14);
    let num_blocks = actual_piece_length.div_ceil(block_size);
    for idx in 0..num_blocks {
        let mut request_msg = [0u8; 17];
        request_msg[0..4].copy_from_slice(&13u32.to_be_bytes());
        request_msg[4] = 6u8;
        request_msg[5..9].copy_from_slice(&piece_idx.to_be_bytes());
        let begin = idx * block_size;
        request_msg[9..13].copy_from_slice(&begin.to_be_bytes());
        let length = if idx == num_blocks - 1 {
            actual_piece_length - begin
        } else {
            block_size
        };
        request_msg[13..17].copy_from_slice(&length.to_be_bytes());
        stream.write_all(&request_msg).await?;
    }

    let mut piece_buffer = vec![0u8; actual_piece_length as usize];
    let mut bytes_received = 0;
    while bytes_received < actual_piece_length {
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

    let hash_start_idx = piece_idx as usize * 20;
    let piece_hash = &pieces_hash[hash_start_idx..hash_start_idx + 20];
    if piece_hash != generate_hash(&piece_buffer) {
        bail!("Received piece does not match piece hash in the torrent file");
    }
    let offset = piece_idx as u64 * piece_length as u64;
    file.seek(Start(offset)).await?;
    file.write_all(&piece_buffer).await?;
    Ok(())
}
