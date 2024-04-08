use super::state::{Chat, Mode};

#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize)]
#[serde(tag = "type", content = "data")]
pub enum Response {
    #[serde(rename = "ack")]
    Ack,

    #[serde(rename = "nil")]
    Nil,

    #[serde(rename = "error")]
    Error(String),

    #[serde(rename = "exit")]
    Exit(u8),

    #[serde(rename = "transcription")]
    Transcription { content: Option<String>, mode: Mode },
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
            Self::Ack => write!(f, "ACK"),
            Self::Nil => write!(f, "NIL"),
            Self::Error(s) => write!(f, "ERROR {}", s),
            Self::Exit(code) => write!(f, "EXIT {}", code),
            Self::Transcription {
                content: Some(s),
                mode,
            } => match mode {
                Mode::Standard => write!(f, "TX {}", s),
                Mode::Clipboard => write!(f, "TX_CLIP {}", s),
                Mode::LiveTyping => write!(f, "TX_LIVE {}", s),
                Mode::Chat(Chat::StartNew(system)) => write!(f, "TX_CHAT {} {}", system, s),
                Mode::Chat(Chat::Continue) => write!(f, "TX_CHAT {}", s),
            },
            Self::Transcription { content: None, .. } => write!(f, "TX_EMPTY"),
        }
    }
}
