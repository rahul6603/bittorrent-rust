use rand::Rng;
use sha1::{Digest, Sha1};

pub(crate) fn generate_peer_id() -> [u8; 20] {
    let mut random_bytes = [0u8; 20];
    rand::rng().fill_bytes(&mut random_bytes);
    random_bytes
}

pub(crate) fn generate_hash(bencoded_info: &[u8]) -> [u8; 20] {
    Sha1::digest(bencoded_info).into()
}
