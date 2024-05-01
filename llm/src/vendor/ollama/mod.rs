mod list;

pub use list::list_models;

const OLLAMA_API: &str = "http://localhost:11434/api";

use std::collections::HashMap;

use derive_builder::Builder;
use serde::{Deserialize, Serialize};

use super::openai::compat::{self, raw_completion, Chat};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    enabled: bool,

    #[serde(flatten, default)]
    host: Host,
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

#[derive(Serialize, Deserialize)]
pub struct Response {
    message: compat::Message,
    created_at: String,
    done: bool,
    eval_count: i32,
    eval_duration: i64,
    load_duration: i64,
    model: String,
    prompt_eval_count: i32,
    prompt_eval_duration: i64,
    total_duration: i64,

    #[serde(flatten)]
    x_groq: XGroq,
}

#[derive(Serialize, Deserialize)]
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
            created: response.created_at.parse().unwrap(),
            // TODO make this a uuid or something
            id: response.x_groq.id.clone(),
            model: response.model,
            object: "chat".to_string(),
            system_fingerprint: response.x_groq.id.clone(),
            usage: compat::Usage {
                completion_time: response.eval_duration as f32,
                completion_tokens: response.eval_count,
                prompt_time: response.prompt_eval_duration as f32,
                prompt_tokens: response.prompt_eval_count,
                total_time: response.total_duration as f32,
                total_tokens: response.eval_count + response.prompt_eval_count,
            },
            meta: HashMap::new(),
        }
    }
}

pub async fn completion(
    system_message: Option<String>,
    user_message: String,
) -> Result<compat::Response, anyhow::Error> {
    let system_message =
        system_message.unwrap_or_else(|| "You are a helpful assistant.".to_string());
    let req = Request::builder()
        .messages(Chat::start_new(system_message, user_message))
        .build()?;
    let req_json = serde_json::to_value(req)?;
    log::info!("req_json: {:?}", req_json);
    let api_base = format!("{OLLAMA_API}/chat");
    let ollama_response: Response = raw_completion(&api_base, None, &req_json)
        .await?
        .json()
        .await?;
    Ok(ollama_response.into())
}
