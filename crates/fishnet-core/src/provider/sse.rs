use serde_json::Value;

use super::openai::failure;
use crate::protocol::OperationError;

/// Bounded SSE framing, including CRLF and UTF-8 split across HTTP chunks.
#[derive(Debug, Default)]
pub(super) struct Decoder {
    line: Vec<u8>,
    data: Vec<String>,
    total: usize,
    events: usize,
}

impl Decoder {
    pub(super) fn push(&mut self, bytes: &[u8]) -> Result<Vec<Value>, OperationError> {
        self.total += bytes.len();
        if self.total > 1_048_576 {
            return Err(failure("provider stream exceeds 1 MiB bound"));
        }
        let mut output = Vec::new();
        for byte in bytes {
            if *byte == b'\n' {
                self.finish_line(&mut output)?;
            } else {
                self.line.push(*byte);
                if self.line.len() > 65_536 {
                    return Err(failure("provider SSE line exceeds bound"));
                }
            }
        }
        Ok(output)
    }

    fn finish_line(&mut self, output: &mut Vec<Value>) -> Result<(), OperationError> {
        if self.line.last() == Some(&b'\r') {
            self.line.pop();
        }
        let line = String::from_utf8(std::mem::take(&mut self.line))
            .map_err(|_| failure("invalid UTF-8 in provider stream"))?;
        if let Some(data) = line.strip_prefix("data:") {
            self.data
                .push(data.strip_prefix(' ').unwrap_or(data).into());
            if self.data.iter().map(String::len).sum::<usize>() > 65_536 {
                return Err(failure("provider event exceeds bound"));
            }
        } else if line.is_empty() && !self.data.is_empty() {
            let data = std::mem::take(&mut self.data).join("\n");
            if data == "[DONE]" {
                return Ok(());
            }
            self.events += 1;
            if self.events > 4_096 {
                return Err(failure("provider stream event count exceeds bound"));
            }
            output.push(
                serde_json::from_str(&data)
                    .map_err(|_| failure("invalid JSON in provider stream"))?,
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Decoder;

    #[test]
    fn split_utf8_crlf_multiline_data_and_comments_are_framed()
    -> Result<(), Box<dyn std::error::Error>> {
        let bytes =
            ": keepalive\r\ndata: {\"type\":\"event\",\r\ndata: \"text\":\"é\"}\r\n\r\n".as_bytes();
        let mut decoder = Decoder::default();
        let mut events = Vec::new();
        for byte in bytes {
            events.extend(decoder.push(std::slice::from_ref(byte))?);
        }
        assert_eq!(events.len(), 1);
        assert_eq!(
            events
                .first()
                .and_then(|event| event.get("text"))
                .and_then(serde_json::Value::as_str),
            Some("é")
        );
        Ok(())
    }

    #[test]
    fn oversized_and_malformed_stream_events_fail() {
        assert!(Decoder::default().push(b"data: malformed\n\n").is_err());
        assert!(Decoder::default().push(&vec![b'x'; 65_537]).is_err());
    }
}
