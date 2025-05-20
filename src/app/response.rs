use super::state::Mode;

#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Response {
    #[serde(rename = "ack")]
    Ack(u128),

    #[serde(rename = "nil")]
    Nil,

    #[serde(rename = "error")]
    Error(String),

    #[serde(rename = "exit")]
    Exit(u8),

    #[serde(rename = "new_mode")]
    NewMode(Mode),

    #[serde(rename = "transcription")]
    Transcription { content: Option<String>, mode: Mode },
}

impl Response {
    /// # Panics
    ///
    /// when `std::time::SystemTime::now()` is earlier than `std::time::UNIX_EPOCH`
    #[must_use]
    pub fn ack() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        Self::Ack(now)
    }
}

impl From<sttx::Timing> for Response {
    fn from(t: sttx::Timing) -> Self {
        Self::Transcription {
            content: Some(t.content().to_string()),
            mode: Mode::default(),
        }
    }
}

impl std::fmt::Display for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Ack(n) => write!(f, "ACK {n}"),
            Self::Nil => write!(f, "NIL"),
            Self::Error(s) => write!(f, "ERROR {s}"),
            Self::Exit(code) => write!(f, "EXIT {code}"),
            Self::NewMode(mode) => write!(f, "NEW_MODE {mode}"),
            Self::Transcription {
                content: Some(s),
                mode,
            } => match mode {
                Mode::Standard => write!(f, "TX {s}"),
                Mode::LiveTyping => write!(f, "TX_LIVE {s}"),
            },
            Self::Transcription { content: None, .. } => write!(f, "TX_EMPTY"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_serialize_ack() {
        let response = Response::Ack(12345);
        let expected = r#"{"type":"ack","data":12345}"#;
        let serialized = serde_json::to_string(&response).unwrap();
        assert_eq!(serialized, expected);
    }

    #[test]
    fn test_serialize_nil() {
        let response = Response::Nil;
        let expected = r#"{"type":"nil"}"#;
        let serialized = serde_json::to_string(&response).unwrap();
        assert_eq!(serialized, expected);
    }

    #[test]
    fn test_serialize_error() {
        let response = Response::Error("Something went wrong".to_string());
        let expected = r#"{"type":"error","data":"Something went wrong"}"#;
        let serialized = serde_json::to_string(&response).unwrap();
        assert_eq!(serialized, expected);
    }

    #[test]
    fn test_serialize_exit() {
        let response = Response::Exit(1);
        let expected = r#"{"type":"exit","data":1}"#;
        let serialized = serde_json::to_string(&response).unwrap();
        assert_eq!(serialized, expected);
    }

    #[test]
    fn test_serialize_new_mode() {
        let response = Response::NewMode(Mode::Standard);
        let expected = r#"{"type":"new_mode","data":{"type":"standard"}}"#;
        let serialized = serde_json::to_string(&response).unwrap();
        assert_eq!(serialized, expected);
    }

    #[test]
    fn test_serialize_transcription_with_content() {
        let response = Response::Transcription {
            content: Some("hello world".to_string()),
            mode: Mode::Standard,
        };
        let expected = r#"{"type":"transcription","data":{"content":"hello world","mode":{"type":"standard"}}}"#;
        let serialized = serde_json::to_string(&response).unwrap();
        assert_eq!(serialized, expected);
    }

    #[test]
    fn test_serialize_transcription_without_content() {
        let response = Response::Transcription {
            content: None,
            mode: Mode::LiveTyping,
        };
        let expected =
            r#"{"type":"transcription","data":{"content":null,"mode":{"type":"live_typing"}}}"#;
        let serialized = serde_json::to_string(&response).unwrap();
        assert_eq!(serialized, expected);
    }
}
