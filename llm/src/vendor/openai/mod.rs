pub mod compat;

use self::compat::Response;
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Copy, Deserialize, Serialize)]
pub enum Model {
    #[serde(rename = "gpt-3.5-turbo-0125")]
    Gpt3_5,
    #[serde(rename = "gpt-4")]
    Gpt4,

    #[default]
    #[serde(rename = "gpt-4-turbo")]
    Gpt4Turbo,
}

const OPENAI_CHAT_API: &str = "https://api.openai.com/v1/chat/completions";
pub async fn completion(
    api_key: String,
    system_message: Option<String>,
    user_message: String,
) -> Result<Response, anyhow::Error> {
    compat::completion(
        OPENAI_CHAT_API,
        api_key,
        Model::default(),
        system_message,
        user_message,
    )
    .await
}
