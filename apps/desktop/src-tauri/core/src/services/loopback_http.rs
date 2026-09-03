//! A POST to a server on this machine's loopback interface, and nothing else.
//!
//! Written out rather than pulled in. The one caller is the Claude channel,
//! which talks to `127.0.0.1:<port>` over plain HTTP with a bearer token — no
//! TLS, no redirects, no proxies, no cookies, no DNS. An HTTP client that
//! handles all of that would be a megabyte of dependencies in a binary that
//! gets uploaded to someone else's machine, to do something a hundred lines can
//! do correctly.
//!
//! Correctly is the operative word: `Connection: close` plus read-to-EOF is
//! the simple path, and chunked responses are still decoded, because a server
//! is entitled to send one whatever we asked for.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct LoopbackResponse {
    pub status: u16,
    pub body: String,
}

#[derive(Debug)]
pub enum LoopbackError {
    Connect(String),
    Timeout,
    Io(String),
    Malformed(String),
}

impl std::fmt::Display for LoopbackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connect(detail) => write!(f, "{detail}"),
            Self::Timeout => write!(f, "the channel did not answer in time"),
            Self::Io(detail) => write!(f, "{detail}"),
            Self::Malformed(detail) => write!(f, "the channel answered with {detail}"),
        }
    }
}

/// Refuses a header value that could inject another header.
///
/// The token comes out of a file on disk, so it is not user input in the usual
/// sense — but a CR or LF in it would end the header and start a new one, and
/// declining is cheaper than reasoning about what that would let through.
fn is_header_safe(value: &str) -> bool {
    !value.contains(['\r', '\n'])
}

/// POSTs `body` to `127.0.0.1:{port}{path}` with a bearer token.
pub async fn post(
    port: u16,
    path: &str,
    token: &str,
    body: &str,
    timeout: Duration,
) -> Result<LoopbackResponse, LoopbackError> {
    if !is_header_safe(token) {
        return Err(LoopbackError::Malformed(
            "a token containing a line break".into(),
        ));
    }

    let exchange = async {
        let mut stream = TcpStream::connect(("127.0.0.1", port))
            .await
            .map_err(|error| LoopbackError::Connect(error.to_string()))?;

        let request = format!(
            "POST {path} HTTP/1.1\r\n\
             Host: 127.0.0.1:{port}\r\n\
             Authorization: Bearer {token}\r\n\
             Content-Type: text/plain; charset=utf-8\r\n\
             Content-Length: {len}\r\n\
             Connection: close\r\n\
             \r\n",
            len = body.len()
        );
        stream
            .write_all(request.as_bytes())
            .await
            .map_err(|error| LoopbackError::Io(error.to_string()))?;
        stream
            .write_all(body.as_bytes())
            .await
            .map_err(|error| LoopbackError::Io(error.to_string()))?;
        stream
            .flush()
            .await
            .map_err(|error| LoopbackError::Io(error.to_string()))?;

        // `Connection: close` means the server closes when it is done, so
        // read-to-EOF is the framing. Bounded anyway: this endpoint answers
        // with a short JSON object, and an unbounded read from a misbehaving
        // server is a memory leak waiting for a bad day.
        let mut raw = Vec::new();
        let mut limited = (&mut stream).take(64 * 1024);
        limited
            .read_to_end(&mut raw)
            .await
            .map_err(|error| LoopbackError::Io(error.to_string()))?;
        parse(&raw)
    };

    match tokio::time::timeout(timeout, exchange).await {
        Ok(result) => result,
        Err(_) => Err(LoopbackError::Timeout),
    }
}

/// Splits a response into its status and its body, decoding chunked transfer
/// encoding where the server used it.
pub(crate) fn parse(raw: &[u8]) -> Result<LoopbackResponse, LoopbackError> {
    let text = String::from_utf8_lossy(raw);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| LoopbackError::Malformed("no header terminator".into()))?;

    let mut lines = head.split("\r\n");
    let status_line = lines
        .next()
        .ok_or_else(|| LoopbackError::Malformed("an empty response".into()))?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| LoopbackError::Malformed(format!("the status line {status_line:?}")))?;

    let chunked = lines.any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case("transfer-encoding")
                && value.to_ascii_lowercase().contains("chunked")
        })
    });

    Ok(LoopbackResponse {
        status,
        body: if chunked {
            dechunk(body)?
        } else {
            body.to_owned()
        },
    })
}

/// Reassembles a chunked body. Chunk extensions after a `;` are ignored, which
/// is what the spec says to do with ones you do not understand.
fn dechunk(body: &str) -> Result<String, LoopbackError> {
    let mut out = String::new();
    let mut rest = body;
    loop {
        let (header, tail) = rest
            .split_once("\r\n")
            .ok_or_else(|| LoopbackError::Malformed("a truncated chunk header".into()))?;
        let size_text = header.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|_| LoopbackError::Malformed(format!("the chunk size {size_text:?}")))?;
        if size == 0 {
            return Ok(out);
        }
        if tail.len() < size {
            return Err(LoopbackError::Malformed("a truncated chunk".into()));
        }
        out.push_str(&tail[..size]);
        // Each chunk is followed by its own CRLF before the next header.
        rest = tail[size..].strip_prefix("\r\n").unwrap_or(&tail[size..]);
    }
}

#[cfg(test)]
mod tests {
    use super::{dechunk, is_header_safe, parse};

    #[test]
    fn a_plain_response_yields_its_status_and_body() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 18\r\n\r\n{\"messageId\":\"a\"}\n";

        let response = parse(raw).expect("parsed");

        assert_eq!(response.status, 200);
        assert!(response.body.contains("messageId"));
    }

    #[test]
    fn a_chunked_response_is_reassembled() {
        // A server is entitled to chunk even when we asked it to close.
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
                    a\r\n{\"messageI\r\n8\r\nd\":\"a\"}\n\r\n0\r\n\r\n";

        let response = parse(raw).expect("parsed");

        assert_eq!(response.status, 200);
        assert_eq!(response.body, "{\"messageId\":\"a\"}\n");
    }

    #[test]
    fn a_chunk_extension_is_ignored_rather_than_parsed_as_a_size() {
        assert_eq!(dechunk("5;name=value\r\nhello\r\n0\r\n\r\n").expect("dechunked"), "hello");
    }

    #[test]
    fn an_error_status_is_still_a_parsed_response() {
        let raw = b"HTTP/1.1 401 Unauthorized\r\n\r\nnope";

        let response = parse(raw).expect("parsed");

        assert_eq!(response.status, 401);
        assert_eq!(response.body, "nope");
    }

    #[test]
    fn a_response_with_no_header_terminator_is_refused() {
        assert!(parse(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n").is_err());
    }

    #[test]
    fn a_token_that_could_forge_a_header_is_refused() {
        assert!(is_header_safe("sk-abc123"));
        assert!(!is_header_safe("abc\r\nX-Admin: true"));
        assert!(!is_header_safe("abc\ndef"));
    }
}
