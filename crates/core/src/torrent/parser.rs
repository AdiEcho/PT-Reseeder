use std::path::Path;
use sha1::{Digest as Sha1Digest, Sha1};
use crate::error::CoreError;
use super::bencode;
use super::models::{TorrentFile, TorrentMeta};

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Parse a .torrent file from disk
pub fn parse_file(path: &Path) -> Result<TorrentMeta, CoreError> {
    let data = std::fs::read(path)
        .map_err(|e| CoreError::Internal(format!("Failed to read torrent file: {}", e)))?;
    parse_bytes(&data)
}

/// Parse torrent metadata from raw bytes
pub fn parse_bytes(data: &[u8]) -> Result<TorrentMeta, CoreError> {
    let value = bencode::decode(data)?;
    let dict = value
        .as_dict()
        .ok_or_else(|| CoreError::Internal("Torrent is not a dict".into()))?;

    // Extract info dict
    let info = dict
        .get(b"info" as &[u8])
        .ok_or_else(|| CoreError::Internal("Missing info dict".into()))?;
    let info_dict = info
        .as_dict()
        .ok_or_else(|| CoreError::Internal("info is not a dict".into()))?;

    // Compute info_hash = SHA1(bencode(info_dict))
    let info_bytes = bencode::encode(info);
    let info_hash = hex_encode(&Sha1::digest(&info_bytes));

    // Extract pieces field and compute pieces_hash = SHA1(info.pieces)
    let pieces = info_dict
        .get(b"pieces" as &[u8])
        .and_then(|v| v.as_bytes())
        .ok_or_else(|| CoreError::Internal("Missing pieces".into()))?;
    let pieces_hash = hex_encode(&Sha1::digest(pieces));

    // Extract name
    let name = info_dict
        .get(b"name" as &[u8])
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    // Extract piece length
    let piece_length = info_dict
        .get(b"piece length" as &[u8])
        .and_then(|v| v.as_int())
        .unwrap_or(0) as u64;

    // Extract files (multi-file) or length (single-file)
    let (files, total_size) =
        if let Some(files_list) = info_dict.get(b"files" as &[u8]).and_then(|v| v.as_list()) {
            // Multi-file torrent
            let mut parsed_files = Vec::new();
            let mut total = 0u64;
            for f in files_list {
                if let Some(fd) = f.as_dict() {
                    let length = fd
                        .get(b"length" as &[u8])
                        .and_then(|v| v.as_int())
                        .unwrap_or(0) as u64;
                    let path: Vec<String> = fd
                        .get(b"path" as &[u8])
                        .and_then(|v| v.as_list())
                        .map(|list| {
                            list.iter()
                                .filter_map(|p| p.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    total += length;
                    parsed_files.push(TorrentFile { path, length });
                }
            }
            (parsed_files, total)
        } else {
            // Single-file torrent
            let length = info_dict
                .get(b"length" as &[u8])
                .and_then(|v| v.as_int())
                .unwrap_or(0) as u64;
            (
                vec![TorrentFile {
                    path: vec![name.clone()],
                    length,
                }],
                length,
            )
        };

    let pieces_count = if piece_length > 0 {
        (total_size + piece_length - 1) / piece_length
    } else {
        0
    };

    // Extract announce
    let announce = dict
        .get(b"announce" as &[u8])
        .and_then(|v| v.as_str())
        .map(String::from);

    // Extract announce-list
    let announce_list = dict
        .get(b"announce-list" as &[u8])
        .and_then(|v| v.as_list())
        .map(|tiers| {
            tiers
                .iter()
                .filter_map(|tier| {
                    tier.as_list().map(|urls| {
                        urls.iter()
                            .filter_map(|u| u.as_str().map(String::from))
                            .collect()
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(TorrentMeta {
        info_hash,
        pieces_hash,
        name,
        total_size,
        files,
        announce,
        announce_list,
        piece_length,
        pieces_count,
    })
}
