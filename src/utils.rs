use rand::Rng;
use serde_bencode::{self, value::Value as BencodeValue};
use serde_json::{Value as JsonValue, json};
use sha1::{Digest, Sha1};

pub(crate) fn generate_peer_id() -> [u8; 20] {
    let mut random_bytes = [0u8; 20];
    rand::rng().fill_bytes(&mut random_bytes);
    random_bytes
}

pub(crate) fn bencode_to_json(bencoded_contents: BencodeValue) -> JsonValue {
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

pub(crate) fn generate_hash(bencoded_info: &[u8]) -> [u8; 20] {
    Sha1::digest(bencoded_info).into()
}
