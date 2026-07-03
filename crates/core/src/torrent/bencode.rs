use std::collections::BTreeMap;
use crate::error::CoreError;

#[derive(Debug, Clone, PartialEq)]
pub enum BencodeValue {
    Int(i64),
    Bytes(Vec<u8>),
    List(Vec<BencodeValue>),
    Dict(BTreeMap<Vec<u8>, BencodeValue>),
}

impl BencodeValue {
    pub fn as_int(&self) -> Option<i64> {
        match self {
            BencodeValue::Int(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            BencodeValue::Bytes(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            BencodeValue::Bytes(v) => std::str::from_utf8(v).ok(),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[BencodeValue]> {
        match self {
            BencodeValue::List(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_dict(&self) -> Option<&BTreeMap<Vec<u8>, BencodeValue>> {
        match self {
            BencodeValue::Dict(v) => Some(v),
            _ => None,
        }
    }
}

/// Decode bencode bytes into a BencodeValue
pub fn decode(data: &[u8]) -> Result<BencodeValue, CoreError> {
    let (value, _) = decode_value(data, 0)?;
    Ok(value)
}

/// Encode a BencodeValue back to bytes
pub fn encode(value: &BencodeValue) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_value(value, &mut buf);
    buf
}

fn encode_value(value: &BencodeValue, buf: &mut Vec<u8>) {
    match value {
        BencodeValue::Int(n) => {
            buf.push(b'i');
            buf.extend_from_slice(n.to_string().as_bytes());
            buf.push(b'e');
        }
        BencodeValue::Bytes(data) => {
            buf.extend_from_slice(data.len().to_string().as_bytes());
            buf.push(b':');
            buf.extend_from_slice(data);
        }
        BencodeValue::List(items) => {
            buf.push(b'l');
            for item in items {
                encode_value(item, buf);
            }
            buf.push(b'e');
        }
        BencodeValue::Dict(map) => {
            buf.push(b'd');
            // BTreeMap iterates in sorted order, which is required by bencode spec
            for (key, val) in map {
                buf.extend_from_slice(key.len().to_string().as_bytes());
                buf.push(b':');
                buf.extend_from_slice(key);
                encode_value(val, buf);
            }
            buf.push(b'e');
        }
    }
}

fn decode_value(data: &[u8], pos: usize) -> Result<(BencodeValue, usize), CoreError> {
    if pos >= data.len() {
        return Err(CoreError::Internal("Unexpected end of bencode data".into()));
    }

    match data[pos] {
        b'i' => decode_int(data, pos),
        b'l' => decode_list(data, pos),
        b'd' => decode_dict(data, pos),
        b'0'..=b'9' => decode_bytes(data, pos),
        ch => Err(CoreError::Internal(format!(
            "Invalid bencode byte '{}' at position {}",
            ch as char, pos
        ))),
    }
}

fn decode_int(data: &[u8], pos: usize) -> Result<(BencodeValue, usize), CoreError> {
    // Format: i<integer>e
    let start = pos + 1; // skip 'i'
    let end = data[start..]
        .iter()
        .position(|&b| b == b'e')
        .map(|p| start + p)
        .ok_or_else(|| CoreError::Internal("Unterminated integer".into()))?;

    let num_str = std::str::from_utf8(&data[start..end])
        .map_err(|_| CoreError::Internal("Invalid integer encoding".into()))?;
    let num: i64 = num_str
        .parse()
        .map_err(|_| CoreError::Internal(format!("Invalid integer: {}", num_str)))?;

    Ok((BencodeValue::Int(num), end + 1))
}

fn decode_bytes(data: &[u8], pos: usize) -> Result<(BencodeValue, usize), CoreError> {
    // Format: <length>:<data>
    let colon = data[pos..]
        .iter()
        .position(|&b| b == b':')
        .map(|p| pos + p)
        .ok_or_else(|| CoreError::Internal("Missing colon in byte string".into()))?;

    let len_str = std::str::from_utf8(&data[pos..colon])
        .map_err(|_| CoreError::Internal("Invalid byte string length".into()))?;
    let len: usize = len_str
        .parse()
        .map_err(|_| CoreError::Internal(format!("Invalid byte string length: {}", len_str)))?;

    let start = colon + 1;
    let end = start + len;
    if end > data.len() {
        return Err(CoreError::Internal("Byte string exceeds data length".into()));
    }

    Ok((BencodeValue::Bytes(data[start..end].to_vec()), end))
}

fn decode_list(data: &[u8], pos: usize) -> Result<(BencodeValue, usize), CoreError> {
    // Format: l<values>e
    let mut items = Vec::new();
    let mut cur = pos + 1; // skip 'l'

    loop {
        if cur >= data.len() {
            return Err(CoreError::Internal("Unterminated list".into()));
        }
        if data[cur] == b'e' {
            return Ok((BencodeValue::List(items), cur + 1));
        }
        let (value, next) = decode_value(data, cur)?;
        items.push(value);
        cur = next;
    }
}

fn decode_dict(data: &[u8], pos: usize) -> Result<(BencodeValue, usize), CoreError> {
    // Format: d<key><value>...e  (keys are byte strings in sorted order)
    let mut map = BTreeMap::new();
    let mut cur = pos + 1; // skip 'd'

    loop {
        if cur >= data.len() {
            return Err(CoreError::Internal("Unterminated dict".into()));
        }
        if data[cur] == b'e' {
            return Ok((BencodeValue::Dict(map), cur + 1));
        }
        // Key must be a byte string
        let (key_val, next) = decode_bytes(data, cur)?;
        let key = match key_val {
            BencodeValue::Bytes(k) => k,
            _ => return Err(CoreError::Internal("Dict key is not a byte string".into())),
        };
        let (value, next) = decode_value(data, next)?;
        map.insert(key, value);
        cur = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_int() {
        let val = BencodeValue::Int(42);
        let encoded = encode(&val);
        assert_eq!(encoded, b"i42e");
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, val);
    }

    #[test]
    fn test_roundtrip_negative_int() {
        let val = BencodeValue::Int(-7);
        let encoded = encode(&val);
        assert_eq!(encoded, b"i-7e");
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, val);
    }

    #[test]
    fn test_roundtrip_bytes() {
        let val = BencodeValue::Bytes(b"hello".to_vec());
        let encoded = encode(&val);
        assert_eq!(encoded, b"5:hello");
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, val);
    }

    #[test]
    fn test_roundtrip_empty_bytes() {
        let val = BencodeValue::Bytes(vec![]);
        let encoded = encode(&val);
        assert_eq!(encoded, b"0:");
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, val);
    }

    #[test]
    fn test_roundtrip_list() {
        let val = BencodeValue::List(vec![
            BencodeValue::Int(1),
            BencodeValue::Bytes(b"two".to_vec()),
            BencodeValue::Int(3),
        ]);
        let encoded = encode(&val);
        assert_eq!(encoded, b"li1e3:twoi3ee");
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, val);
    }

    #[test]
    fn test_roundtrip_dict_sorted_keys() {
        let mut map = BTreeMap::new();
        map.insert(b"zebra".to_vec(), BencodeValue::Int(1));
        map.insert(b"apple".to_vec(), BencodeValue::Int(2));
        let val = BencodeValue::Dict(map);

        let encoded = encode(&val);
        // Keys must be sorted: "apple" before "zebra"
        assert_eq!(encoded, b"d5:applei2e5:zebrai1ee");

        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, val);
    }

    #[test]
    fn test_nested_structure() {
        let mut inner_dict = BTreeMap::new();
        inner_dict.insert(b"key".to_vec(), BencodeValue::Bytes(b"val".to_vec()));

        let val = BencodeValue::List(vec![
            BencodeValue::Dict(inner_dict),
            BencodeValue::List(vec![BencodeValue::Int(99)]),
        ]);

        let encoded = encode(&val);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, val);
    }

    #[test]
    fn test_dict_keys_are_sorted_in_output() {
        let mut map = BTreeMap::new();
        map.insert(b"c".to_vec(), BencodeValue::Int(3));
        map.insert(b"a".to_vec(), BencodeValue::Int(1));
        map.insert(b"b".to_vec(), BencodeValue::Int(2));
        let val = BencodeValue::Dict(map);

        let encoded = encode(&val);
        // Verify byte-level sorted order: a, b, c
        assert_eq!(encoded, b"d1:ai1e1:bi2e1:ci3ee");
    }
}
