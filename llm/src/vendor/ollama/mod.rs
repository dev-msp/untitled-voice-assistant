mod list;

use chrono::{offset::Local, DateTime};
use derive_builder::Builder;
use serde::{Deserialize, Serialize};

use std::collections::HashMap;

use crate::vendor::openai::compat::FallibleResponse;

use super::openai::compat::{self, raw_completion, Chat};

pub use list::list_models;

const OLLAMA_API: &str = "http://localhost:11434/api";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    enabled: bool,

    #[serde(flatten, default)]
    host: Host,

    default_model: Option<String>,
}

impl Provider {
    pub fn default_model(&self) -> Option<String> {
        self.default_model.clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    host: String,
    port: u16,
}

impl Default for Host {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 11434,
        }
    }
}

#[derive(Serialize, Deserialize, Builder)]
pub struct RequestOptions {
    #[builder(setter(strip_option), default)]
    temperature: Option<f32>,

    #[builder(setter(strip_option), default)]
    top_p: Option<f32>,
}

impl Default for RequestOptions {
    fn default() -> Self {
        Self {
            temperature: Some(0.5),
            top_p: None,
        }
    }
}

#[derive(Serialize, Deserialize, Builder)]
pub struct Request {
    #[builder(setter(strip_option), default)]
    messages: compat::Chat,

    #[builder(default = "String::from(\"dolphin-llama3:latest\")")]
    model: String,

    #[builder(setter(strip_option), default)]
    max_tokens: Option<i32>,

    #[builder(default = "false")]
    stream: bool,

    #[builder(setter(strip_option), default)]
    stop: Option<String>,
}

impl Request {
    pub fn builder() -> RequestBuilder {
        RequestBuilder::default()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    message: compat::Message,
    role: compat::Role,
    created_at: String,
    done: bool,
    eval_count: i32,
    eval_duration: i64,
    load_duration: i64,
    model: String,
    total_duration: i64,

    #[serde(flatten)]
    x_groq: XGroq,

    #[serde(flatten)]
    rest: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct XGroq {
    #[serde(flatten)]
    id: String,
}

impl From<Response> for compat::Response {
    fn from(response: Response) -> Self {
        compat::Response {
            choices: vec![compat::Choice {
                finish_reason: "completed".to_string(),
                index: 0,
                logprobs: None,
                message: response.message,
            }],
            created: DateTime::parse_from_rfc3339(&response.created_at)
                .map(|dt| dt.with_timezone(&Local))
                .unwrap_or_else(|_| {
                    log::warn!("failed to parse date: {}", response.created_at);
                    Local::now()
                })
                .timestamp(),
            // TODO make this a uuid or something
            id: response.x_groq.id.clone(),
            model: response.model,
            object: "chat".to_string(),
            system_fingerprint: response.x_groq.id.clone(),
            usage: Some(compat::Usage {
                completion_tokens: response.eval_count,
                prompt_tokens: 0,
                total_tokens: response.eval_count,
            }),
        }
    }
}

pub async fn completion(
    model: String,
    system_message: String,
    user_message: String,
) -> Result<compat::Response, anyhow::Error> {
    let req = Request::builder()
        .messages(Chat::start_new(system_message, user_message))
        .model(model)
        .build()?;
    let req_json = serde_json::to_value(req)?;
    log::info!("req_json: {:?}", req_json);
    let api_base = format!("{OLLAMA_API}/chat");
    let ollama_response: FallibleResponse<Response> = raw_completion(&api_base, None, &req_json)
        .await?
        .json()
        .await?;

    Ok(Result::from(ollama_response)?.into())
}
