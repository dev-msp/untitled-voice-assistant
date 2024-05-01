use std::collections::{HashMap, VecDeque};

use derive_builder::Builder;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::vendor::get_api_key;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    enabled: bool,
    api_key_command: Vec<String>,
    default_model: Option<String>,
}

impl Provider {
    pub async fn get_api_key(&self) -> anyhow::Result<Option<String>> {
        get_api_key(&self.api_key_command).await
    }
}

pub async fn completion<M>(
    api_base: &'static str,
    api_key: String,
    model: M,
    system_message: Option<String>,
    user_message: String,
) -> Result<Response, anyhow::Error>
where
    M: Serialize + Default + Clone,
{
    let system_message =
        system_message.unwrap_or_else(|| "You are a helpful assistant.".to_string());

    let req = RequestBuilder::default()
        .messages(Chat::start_new(system_message, user_message))
        .model(model)
        .build()?;

    Ok(
        raw_completion(api_base, Some(api_key), &serde_json::to_value(req)?)
            .await?
            .json()
            .await?,
    )
}

pub(crate) async fn raw_completion(
    api_base: &str,
    api_key: Option<String>,
    req: &serde_json::Value,
) -> Result<reqwest::Response, anyhow::Error> {
    let mut req_builder = reqwest::Client::new().post(api_base);
    if let Some(api_key) = api_key {
        req_builder = req_builder.bearer_auth(api_key);
    }

    let response: reqwest::Response = req_builder.json(req).send().await?;

    Ok(response)
}

#[derive(Serialize, Deserialize, Builder)]
pub struct Request<M> {
    #[builder(setter(strip_option), default)]
    messages: Chat,

    model: M,

    #[builder(setter(strip_option), default)]
    temperature: Option<f32>,

    #[builder(setter(strip_option), default)]
    max_tokens: Option<i32>,

    #[builder(setter(strip_option), default)]
    top_p: Option<f32>,

    #[builder(default = "false")]
    stream: bool,

    #[builder(setter(strip_option), default)]
    stop: Option<String>,
}

impl<M> Default for Request<M>
where
    M: Default,
{
    fn default() -> Self {
        Self {
            messages: Chat::default(),
            model: M::default(),
            temperature: None,
            max_tokens: None,
            top_p: None,
            stream: false,
            stop: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub choices: Vec<Choice>,
    pub created: i64,
    pub id: String,
    pub model: String,
    pub object: String,
    pub system_fingerprint: String,
    pub usage: Usage,

    #[serde(flatten)]
    pub meta: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub finish_reason: String,
    pub index: i32,
    pub logprobs: Option<serde_json::Value>,
    pub message: Message,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub completion_time: f32,
    pub completion_tokens: i32,
    pub prompt_time: f32,
    pub prompt_tokens: i32,
    pub total_time: f32,
    pub total_tokens: i32,
}

impl Response {
    pub fn content(&self) -> String {
        self.choices[0].message.content.clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(into = "VecDeque<Message>", try_from = "VecDeque<Message>")]
pub struct Chat {
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
pub enum ConversionError {
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
        messages.into_iter().collect()
    }
}

impl Chat {
    pub fn new(system: String, messages: VecDeque<Message>) -> Self {
        Self { system, messages }
    }

    pub fn start_new(system: String, user: String) -> Self {
        let mut messages = VecDeque::new();
        messages.push_back(Message::user(user));
        Self::new(system, messages)
    }
}

impl FromIterator<Message> for Result<Chat, ConversionError> {
    fn from_iter<T: IntoIterator<Item = Message>>(iter: T) -> Self {
        let mut iter = iter.into_iter();
        let system = match iter.next() {
            Some(
                ref msg @ Message {
                    role: Role::System, ..
                },
            ) => msg.content.clone(),
            _ => return Err(ConversionError::NoSystem),
        };

        let messages: VecDeque<Message> = iter
            .map(|msg| {
                if msg.role == Role::System {
                    Err(ConversionError::MultipleSystem)
                } else {
                    Ok(msg)
                }
            })
            .try_collect()?;

        if messages.is_empty() {
            return Err(ConversionError::Empty);
        }

        Ok(Chat { system, messages })
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

    pub fn user(message: String) -> Self {
        Self {
            role: Role::User,
            content: message,
        }
    }

    pub fn system(message: String) -> Self {
        Self {
            role: Role::System,
            content: message,
        }
    }

    pub fn assistant(message: String) -> Self {
        Self {
            role: Role::Assistant,
            content: message,
        }
    }
}
