# bittorrent-rust

A minimal BitTorrent client implementation in rust

## Features

- Torrent file parsing
- Tracker communication
- Peer protocol implementation (piece downloading with SHA1 verification)
- Single file download

## How to use

Download a file:
```
cargo run -- download -o <output_path> <torrent_file>
```

Show torrent file info:
```
cargo run -- info <torrent_file>
```

List peers from tracker:
```
cargo run -- peers <torrent_file>
```

To build the binary:
```
cargo build --release
```

## Credits

Built as part of the [CodeCrafters BitTorrent challenge](https://app.codecrafters.io/courses/bittorrent/overview).
