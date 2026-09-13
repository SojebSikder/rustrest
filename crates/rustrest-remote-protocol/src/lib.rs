//! Wire schema and framing shared between the local `rustrest-remote` client
//! and the headless `rustrest-remote-agent` binary that runs on the remote
//! host.

use serde::{Deserialize, Serialize};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// a single remote directory entry, as returned by [`Request::ListDir`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteEntry {
    pub name: String,
    pub is_dir: bool,
    /// file size in bytes; `0` for directories.
    pub size: u64,
    /// last-modified time, seconds since the Unix epoch.
    pub modified: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    ListDir(String),
    ReadFile(String),
    WriteFile(String, Vec<u8>),
    CreateDir(String),
    Delete(String),
    Rename(String, String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    DirListing(Vec<RemoteEntry>),
    FileContent(Vec<u8>),
    Ok,
    Error(String),
}

/// an envelope pairing a request/response with an id, so a client can match
/// out-of-order replies to the request that produced them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub id: u64,
    pub payload: T,
}

fn to_io_err(err: bincode::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, err)
}

/// writes one length-prefixed, bincode-encoded message.
pub async fn write_message<W, T>(writer: &mut W, value: &T) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let bytes = bincode::serialize(value).map_err(to_io_err)?;
    writer.write_u32(bytes.len() as u32).await?;
    writer.write_all(&bytes).await?;
    writer.flush().await
}

/// reads one length-prefixed, bincode-encoded message.
pub async fn read_message<R, T>(reader: &mut R) -> io::Result<T>
where
    R: AsyncRead + Unpin,
    T: for<'de> Deserialize<'de>,
{
    let len = reader.read_u32().await?;
    let mut buf = vec![0u8; len as usize];
    reader.read_exact(&mut buf).await?;
    bincode::deserialize(&buf).map_err(to_io_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn round_trips_request_and_response() {
        let mut buf = Vec::new();
        let req = Envelope {
            id: 7,
            payload: Request::ListDir("/home/dev".to_string()),
        };
        write_message(&mut buf, &req).await.unwrap();

        let mut cursor = std::io::Cursor::new(buf);
        let decoded: Envelope<Request> = read_message(&mut cursor).await.unwrap();
        assert_eq!(decoded.id, 7);
        match decoded.payload {
            Request::ListDir(path) => assert_eq!(path, "/home/dev"),
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn round_trips_dir_listing() {
        let mut buf = Vec::new();
        let resp = Envelope {
            id: 1,
            payload: Response::DirListing(vec![RemoteEntry {
                name: "src".to_string(),
                is_dir: true,
                size: 0,
                modified: 1_700_000_000,
            }]),
        };
        write_message(&mut buf, &resp).await.unwrap();

        let mut cursor = std::io::Cursor::new(buf);
        let decoded: Envelope<Response> = read_message(&mut cursor).await.unwrap();
        match decoded.payload {
            Response::DirListing(entries) => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].name, "src");
                assert!(entries[0].is_dir);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn closed_stream_yields_unexpected_eof() {
        let mut cursor = std::io::Cursor::new(Vec::<u8>::new());
        let err = read_message::<_, Envelope<Request>>(&mut cursor)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }
}
