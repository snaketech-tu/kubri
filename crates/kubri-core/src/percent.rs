//! Percent-encoding for URL query values and path segments.

/// Escapes every byte except the unreserved ones of RFC 3986.
pub(crate) fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn keeps_unreserved_bytes_and_escapes_the_rest() {
        assert_eq!(super::encode("a-Z_0.~"), "a-Z_0.~");
        assert_eq!(super::encode("up{job=\"x\"} ü"), "up%7Bjob%3D%22x%22%7D%20%C3%BC");
    }
}
