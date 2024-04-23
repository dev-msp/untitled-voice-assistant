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
