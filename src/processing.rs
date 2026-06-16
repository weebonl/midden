use bytes::Bytes;

pub fn sniff_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"%PDF-") {
        return Some("application/pdf");
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return Some("application/zip");
    }
    if bytes
        .iter()
        .all(|byte| matches!(*byte, b'\t' | b'\n' | b'\r' | 0x20..=0x7e))
    {
        return Some("text/plain");
    }
    Some("application/octet-stream")
}

pub fn file_metadata_json(
    content_type: &str,
    size_bytes: i64,
    image_dimensions: Option<(i64, i64)>,
    metadata_stripped: bool,
) -> anyhow::Result<String> {
    Ok(serde_json::to_string(&serde_json::json!({
        "content_type": content_type,
        "size_bytes": size_bytes,
        "image_width": image_dimensions.map(|(width, _)| width),
        "image_height": image_dimensions.map(|(_, height)| height),
        "metadata_stripped": metadata_stripped,
    }))?)
}

pub fn strip_file_metadata(content_type: &str, bytes: Bytes) -> Bytes {
    match content_type {
        "image/jpeg" => strip_jpeg_metadata(bytes.as_ref())
            .map(Bytes::from)
            .unwrap_or(bytes),
        "image/png" => strip_png_metadata(bytes.as_ref())
            .map(Bytes::from)
            .unwrap_or(bytes),
        _ => bytes,
    }
}

pub fn thumbnail_hash(content_type: &str, original_hash: &str) -> Option<String> {
    if matches!(content_type, "image/jpeg" | "image/png" | "image/gif") {
        Some(original_hash.to_string())
    } else {
        None
    }
}

fn strip_jpeg_metadata(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() < 4 || bytes[0..2] != [0xff, 0xd8] {
        return None;
    }
    let mut out = bytes[0..2].to_vec();
    let mut offset = 2;
    while offset + 4 <= bytes.len() {
        if bytes[offset] != 0xff {
            out.extend_from_slice(&bytes[offset..]);
            return Some(out);
        }
        let marker = bytes[offset + 1];
        if marker == 0xda || marker == 0xd9 {
            out.extend_from_slice(&bytes[offset..]);
            return Some(out);
        }
        let length = u16::from_be_bytes([bytes[offset + 2], bytes[offset + 3]]) as usize;
        if length < 2 || offset + 2 + length > bytes.len() {
            return None;
        }
        let is_metadata = (0xe0..=0xef).contains(&marker) || marker == 0xfe;
        if !is_metadata {
            out.extend_from_slice(&bytes[offset..offset + 2 + length]);
        }
        offset += 2 + length;
    }
    Some(out)
}

fn strip_png_metadata(bytes: &[u8]) -> Option<Vec<u8>> {
    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 8 || &bytes[0..8] != PNG_SIGNATURE {
        return None;
    }
    let mut out = PNG_SIGNATURE.to_vec();
    let mut offset = 8;
    while offset + 12 <= bytes.len() {
        let length = u32::from_be_bytes(bytes[offset..offset + 4].try_into().ok()?) as usize;
        let chunk_end = offset + 12 + length;
        if chunk_end > bytes.len() {
            return None;
        }
        let chunk_type = &bytes[offset + 4..offset + 8];
        let is_critical = chunk_type[0].is_ascii_uppercase();
        if is_critical {
            out.extend_from_slice(&bytes[offset..chunk_end]);
        }
        offset = chunk_end;
        if chunk_type == b"IEND" {
            break;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_mime_detects_common_fixture_shapes() {
        assert_eq!(sniff_mime(b"\x89PNG\r\n\x1a\nrest"), Some("image/png"));
        assert_eq!(sniff_mime(b"GIF89arest"), Some("image/gif"));
        assert_eq!(sniff_mime(&[0xff, 0xd8, 0xff, 0xe0]), Some("image/jpeg"));
        assert_eq!(sniff_mime(b"plain text"), Some("text/plain"));
    }
}
