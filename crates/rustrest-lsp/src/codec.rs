//! LSP base-protocol framing: `Content-Length: N\r\n\r\n<N bytes of JSON>`.

use serde_json::Value;

/// frames one JSON-RPC message for the server's stdin.
pub fn encode(message: &Value) -> Vec<u8> {
    let body = message.to_string();
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend_from_slice(body.as_bytes());
    out
}

/// incremental decoder for the server's stdout. Chunks can split headers
/// and bodies anywhere, and one chunk can hold several messages.
#[derive(Debug, Default)]
pub struct Decoder {
    buf: Vec<u8>,
}

impl Decoder {
    /// appends `chunk` and returns every message completed by it, in order.
    /// Malformed frames/bodies come back as `Err` and are skipped.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Result<Value, String>> {
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some(header_end) = find(&self.buf, b"\r\n\r\n") {
            let body_start = header_end + 4;
            let len = match content_length(&self.buf[..header_end]) {
                Some(len) => len,
                None => {
                    // no usable header: drop it so we can resync on the next frame
                    let header = String::from_utf8_lossy(&self.buf[..header_end]).into_owned();
                    self.buf.drain(..body_start);
                    out.push(Err(format!("frame without Content-Length: {header:?}")));
                    continue;
                }
            };
            if self.buf.len() < body_start + len {
                break;
            }
            let body: Vec<u8> = self
                .buf
                .drain(..body_start + len)
                .skip(body_start)
                .collect();
            out.push(serde_json::from_slice(&body).map_err(|e| format!("invalid JSON body: {e}")));
        }
        out
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn content_length(header: &[u8]) -> Option<usize> {
    let header = std::str::from_utf8(header).ok()?;
    header.split("\r\n").find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.trim().eq_ignore_ascii_case("content-length") {
            value.trim().parse().ok()
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn roundtrip() {
        let msg = json!({"jsonrpc": "2.0", "id": 1, "result": "héllo"});
        let mut d = Decoder::default();
        let out = d.push(&encode(&msg));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].as_ref().unwrap(), &msg);
    }

    #[test]
    fn content_length_counts_bytes() {
        let bytes = encode(&json!("ü€"));
        let text = String::from_utf8(bytes).unwrap();
        // "\"ü€\"" = 1 + 2 + 3 + 1 bytes
        assert!(text.starts_with("Content-Length: 7\r\n\r\n"));
    }

    #[test]
    fn split_across_every_byte() {
        let a = json!({"id": 1, "result": null});
        let b = json!({"method": "x", "params": {"s": "日本"}});
        let mut all = encode(&a);
        all.extend(encode(&b));
        let mut d = Decoder::default();
        let mut got = Vec::new();
        for byte in &all {
            got.extend(d.push(std::slice::from_ref(byte)));
        }
        let got: Vec<Value> = got.into_iter().map(Result::unwrap).collect();
        assert_eq!(got, vec![a, b]);
    }

    #[test]
    fn several_messages_in_one_chunk_and_extra_headers() {
        let mut all = b"Content-Type: application/vscode-jsonrpc; charset=utf-8\r\ncontent-length: 2\r\n\r\n{}"
            .to_vec();
        all.extend(encode(&json!(1)));
        all.extend(b"Content-Length: 5\r\n\r\n[1,"); // incomplete
        let mut d = Decoder::default();
        let got = d.push(&all);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].as_ref().unwrap(), &json!({}));
        assert_eq!(got[1].as_ref().unwrap(), &json!(1));
        let got = d.push(b"2]");
        assert_eq!(got[0].as_ref().unwrap(), &json!([1, 2]));
    }

    #[test]
    fn resyncs_after_bad_frames() {
        let mut all = b"Bogus: 1\r\n\r\n".to_vec();
        all.extend(b"Content-Length: 3\r\n\r\n{x}");
        all.extend(encode(&json!(true)));
        let mut d = Decoder::default();
        let got = d.push(&all);
        assert_eq!(got.len(), 3);
        assert!(got[0].is_err());
        assert!(got[1].is_err());
        assert_eq!(got[2].as_ref().unwrap(), &json!(true));
    }
}
