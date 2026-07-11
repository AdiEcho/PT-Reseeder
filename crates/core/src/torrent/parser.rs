use super::bencode;
use super::models::{TorrentFile, TorrentMeta};
use crate::error::CoreError;
use sha1::{Digest as Sha1Digest, Sha1};
use std::path::Path;

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

#[cfg(test)]
mod tests {
    use super::*;
    use sha1::{Digest, Sha1};

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    #[test]
    fn parse_bytes_computes_info_hash_and_pieces_hash() {
        let pieces = [7u8; 20];
        let mut data = Vec::new();
        data.extend_from_slice(b"d8:announce23:http://tracker/announce4:infod");
        data.extend_from_slice(
            b"6:lengthi123e4:name10:sample.mkv12:piece lengthi16384e6:pieces20:",
        );
        data.extend_from_slice(&pieces);
        data.extend_from_slice(b"ee");

        let meta = parse_bytes(&data).expect("valid torrent metadata");
        let info_bytes = b"d6:lengthi123e4:name10:sample.mkv12:piece lengthi16384e6:pieces20:\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07\x07e";

        assert_eq!(meta.name, "sample.mkv");
        assert_eq!(meta.total_size, 123);
        assert_eq!(meta.info_hash, hex(&Sha1::digest(info_bytes)));
        assert_eq!(meta.pieces_hash, hex(&Sha1::digest(pieces)));
    }

    #[test]
    fn parse_file_reads_torrent_from_disk() {
        let pieces = [3u8; 20];
        let mut data = Vec::new();
        data.extend_from_slice(b"d4:infod6:lengthi1e4:name1:a12:piece lengthi1e6:pieces20:");
        data.extend_from_slice(&pieces);
        data.extend_from_slice(b"ee");

        let path = std::env::temp_dir().join(format!(
            "pt-reseeder-parser-test-{}.torrent",
            std::process::id()
        ));
        std::fs::write(&path, data).expect("write fixture torrent");
        let meta = parse_file(&path).expect("parse fixture torrent");
        let _ = std::fs::remove_file(&path);

        assert_eq!(meta.name, "a");
        assert_eq!(meta.total_size, 1);
        assert_eq!(meta.pieces_hash, hex(&Sha1::digest(pieces)));
    }

    #[test]
    fn parse_bytes_single_file_torrent_populates_files_list() {
        let pieces = [1u8; 20];
        let mut data = Vec::new();
        data.extend_from_slice(b"d4:infod6:lengthi500e4:name8:test.txt12:piece lengthi256e6:pieces20:");
        data.extend_from_slice(&pieces);
        data.extend_from_slice(b"ee");

        let meta = parse_bytes(&data).unwrap();
        assert_eq!(meta.files.len(), 1);
        assert_eq!(meta.files[0].length, 500);
        assert_eq!(meta.files[0].path, vec!["test.txt".to_string()]);
        assert_eq!(meta.total_size, 500);
        assert_eq!(meta.piece_length, 256);
        assert_eq!(meta.pieces_count, 2); // ceil(500/256)
    }

    #[test]
    fn parse_bytes_multi_file_torrent() {
        let pieces = [2u8; 20];
        let mut data = Vec::new();
        data.extend_from_slice(
            b"d4:infod5:filesld6:lengthi100e4:pathl5:a.txteed6:lengthi200e4:pathl3:dir5:b.txteee4:name7:my_pack12:piece lengthi512e6:pieces20:",
        );
        data.extend_from_slice(&pieces);
        data.extend_from_slice(b"ee");

        let meta = parse_bytes(&data).unwrap();
        assert_eq!(meta.name, "my_pack");
        assert_eq!(meta.total_size, 300);
        assert_eq!(meta.files.len(), 2);
        assert_eq!(meta.files[0].length, 100);
        assert_eq!(meta.files[0].path, vec!["a.txt".to_string()]);
        assert_eq!(meta.files[1].length, 200);
        assert_eq!(meta.files[1].path, vec!["dir".to_string(), "b.txt".to_string()]);
    }

    #[test]
    fn parse_bytes_with_announce_url() {
        let pieces = [0u8; 20];
        let mut data = Vec::new();
        data.extend_from_slice(b"d8:announce30:http://tracker.example.com/ann4:infod6:lengthi10e4:name1:x12:piece lengthi10e6:pieces20:");
        data.extend_from_slice(&pieces);
        data.extend_from_slice(b"ee");

        let meta = parse_bytes(&data).unwrap();
        assert_eq!(meta.announce, Some("http://tracker.example.com/ann".to_string()));
    }

    #[test]
    fn parse_bytes_with_announce_list() {
        let pieces = [0u8; 20];
        let mut data = Vec::new();
        data.extend_from_slice(b"d13:announce-listll19:http://tracker1/annee4:infod6:lengthi10e4:name1:x12:piece lengthi10e6:pieces20:");
        data.extend_from_slice(&pieces);
        data.extend_from_slice(b"ee");

        let meta = parse_bytes(&data).unwrap();
        // announce-list has two tiers, each with one URL
        assert!(!meta.announce_list.is_empty());
    }

    #[test]
    fn parse_bytes_missing_info_dict_returns_error() {
        let data = b"d4:name4:teste";
        let result = parse_bytes(data);
        assert!(result.is_err());
    }

    #[test]
    fn parse_bytes_not_a_dict_returns_error() {
        let data = b"i42e";
        let result = parse_bytes(data);
        assert!(result.is_err());
    }

    #[test]
    fn parse_bytes_missing_pieces_returns_error() {
        let data = b"d4:infod6:lengthi10e4:name1:x12:piece lengthi10eee";
        let result = parse_bytes(data);
        assert!(result.is_err());
    }

    #[test]
    fn parse_bytes_zero_piece_length_sets_pieces_count_zero() {
        let pieces = [0u8; 20];
        let mut data = Vec::new();
        data.extend_from_slice(b"d4:infod6:lengthi100e4:name1:x12:piece lengthi0e6:pieces20:");
        data.extend_from_slice(&pieces);
        data.extend_from_slice(b"ee");

        let meta = parse_bytes(&data).unwrap();
        assert_eq!(meta.pieces_count, 0);
    }

    #[test]
    fn parse_file_nonexistent_returns_error() {
        let result = parse_file(std::path::Path::new("/tmp/nonexistent-torrent-file-xyz.torrent"));
        assert!(result.is_err());
    }
}
