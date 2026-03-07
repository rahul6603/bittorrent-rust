use crate::{torrent::get_torrent_file_info, tracker::get_peer_list, utils::generate_hash};
use anyhow::Result;
use clap::{Parser, Subcommand};
use serde_bencode::{self};

mod download;
mod peer;
mod torrent;
mod tracker;
mod utils;

#[derive(Parser)]
#[command(
    author,
    version,
    about = "A minimal BitTorrent client implementation in Rust"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show torrent file information
    Info {
        /// Path to the torrent file
        torrent_file: String,
    },
    /// List peers for a torrent
    Peers {
        /// Path to the torrent file
        torrent_file: String,
    },
    /// Download a file from a torrent
    Download {
        /// Output file path
        #[arg(short)]
        o: String,
        /// Path to the torrent file
        torrent_file: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Info { torrent_file } => {
            let decoded_contents = get_torrent_file_info(&torrent_file).await?;
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
        Command::Peers { torrent_file } => {
            let peer_addr_list = get_peer_list(&torrent_file).await?;
            for peer_addr in peer_addr_list {
                println!("{peer_addr}");
            }
        }
        Command::Download { o, torrent_file } => {
            download::handle_file_download(&torrent_file, &o).await?;
        }
    }
    Ok(())
}
