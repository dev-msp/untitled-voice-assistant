#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize)]
#[serde(tag = "type", content = "data")]
pub enum Response {
    #[serde(rename = "ack")]
    Ack,

    #[serde(rename = "exit")]
    Exit(u8),

    #[serde(rename = "transcription")]
    Transcription(Option<String>),
}

impl From<sttx::Timing> for Response {
    fn from(t: sttx::Timing) -> Self {
        Self::Transcription(Some(t.content().to_string()))
    }
}

impl std::fmt::Display for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Ack => write!(f, "ACK"),
            Self::Exit(code) => write!(f, "EXIT {}", code),
            Self::Transcription(Some(s)) => write!(f, "TX {}", s),
            Self::Transcription(None) => write!(f, "TX_EMPTY"),
        }
    }
}
