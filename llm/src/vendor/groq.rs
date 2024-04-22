use std::collections::VecDeque;

use itertools::Itertools;
use serde::{Deserialize, Serialize};

const GROQ_CHAT_API: &str = "https://api.groq.com/openai/v1/chat/completions";

const MIXTRAL: &str = "mixtral-8x7b-32768";
const LLAMA3_70B: &str = "llama3-70b-8192";
const LLAMA3_8B: &str = "llama3-8b-8192";

#[derive(Serialize, Deserialize)]
pub struct CompletionRequest {
    messages: Chat,
    model: Model,
    temperature: f32,
    max_tokens: i32,
    top_p: f32,
    stream: bool,
    stop: Option<String>,
}

impl Default for CompletionRequest {
    fn default() -> Self {
        Self {
            messages: Chat::default(),
            model: Model::Llama3_70B,
            temperature: 0.0,
            max_tokens: 0,
            top_p: 0.0,
            stream: false,
            stop: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse(serde_json::Value);

impl std::fmt::Display for CompletionResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(into = "VecDeque<Message>", try_from = "VecDeque<Message>")]
struct Chat {
    system: String,
    messages: VecDeque<Message>,
}

impl Default for Chat {
    fn default() -> Self {
        let msg = Message {
            role: Role::User,
            content: "Hello!".to_string(),
        };
        let mut messages = VecDeque::new();
        messages.push_back(msg);
        Self {
            system: "You are a helpful assistant.".to_string(),
            messages,
        }
    }
}

impl From<Chat> for VecDeque<Message> {
    fn from(chat: Chat) -> Self {
        let mut messages = chat.messages;
        messages.push_front(Message {
            role: Role::System,
            content: chat.system,
        });
        messages
    }
}

#[derive(Debug, Clone, Copy, thiserror::Error)]
enum ConversionError {
    #[error("no system message found")]
    NoSystem,

    #[error("multiple system messages found")]
    MultipleSystem,

    #[error("no user messages found")]
    Empty,
}

impl TryFrom<VecDeque<Message>> for Chat {
    type Error = ConversionError;

    fn try_from(messages: VecDeque<Message>) -> Result<Self, Self::Error> {
        let mut msgs = messages.into_iter();
        let system = match msgs.next() {
            Some(ref msg @ Message { ref content, .. }) if msg.is_system() => content.clone(),
            _ => return Err(ConversionError::NoSystem),
        };

        let messages: VecDeque<Message> = msgs
            .map(|msg| {
                if msg.is_system() {
                    Err(ConversionError::MultipleSystem)
                } else {
                    Ok(msg)
                }
            })
            .try_collect()?;

        if messages.is_empty() {
            return Err(ConversionError::Empty);
        }

        Ok(Self { system, messages })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(try_from = "&str", into = "&str")]
enum Model {
    Mixtral,
    Llama3_70B,
    Llama3_8B,
}

impl TryFrom<&str> for Model {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            MIXTRAL => Ok(Self::Mixtral),
            LLAMA3_70B => Ok(Self::Llama3_70B),
            LLAMA3_8B => Ok(Self::Llama3_8B),
            _ => Err("invalid model"),
        }
    }
}

impl From<Model> for &str {
    fn from(model: Model) -> Self {
        model.api_name()
    }
}

impl Model {
    #[inline]
    fn api_name(&self) -> &'static str {
        match self {
            Model::Mixtral => MIXTRAL,
            Model::Llama3_70B => LLAMA3_70B,
            Model::Llama3_8B => LLAMA3_8B,
        }
    }

    #[inline]
    #[allow(unused)]
    fn max_tokens(&self) -> i32 {
        match self {
            Model::Mixtral => 32768,
            Model::Llama3_70B => 8192,
            Model::Llama3_8B => 8192,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    #[serde(rename = "system")]
    System,

    #[serde(rename = "assistant")]
    Assistant,

    #[serde(rename = "user")]
    User,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    role: Role,
    content: String,
}

impl Message {
    pub fn is_system(&self) -> bool {
        self.role == Role::System
    }
}

pub async fn completion(
    api_key: String,
    system_message: Option<String>,
    user_message: String,
) -> Result<CompletionResponse, anyhow::Error> {
    let system_message =
        system_message.unwrap_or_else(|| "You are a helpful assistant.".to_string());
    let chat = Chat {
        system: system_message,
        messages: vec![Message {
            role: Role::User,
            content: user_message,
        }]
        .into(),
    };
    let req = CompletionRequest {
        messages: chat,
        ..Default::default()
    };

    let response: reqwest::Response = reqwest::Client::new()
        .post(GROQ_CHAT_API)
        .bearer_auth(api_key)
        .json(&req)
        .send()
        .await?;

    let hey: CompletionResponse = response.json().await?;
    Ok(hey)
}
