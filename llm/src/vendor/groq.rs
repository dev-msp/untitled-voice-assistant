use serde::{Deserialize, Serialize};

use super::openai::{self, compat::Response};

pub(crate) const GROQ_CHAT_API: &str = "https://api.groq.com/openai/v1";

const MIXTRAL: &str = "mixtral-8x7b-32768";
const LLAMA3_70B: &str = "llama3-70b-8192";
const LLAMA3_8B: &str = "llama3-8b-8192";

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
#[serde(try_from = "&str", into = "&str")]
enum Model {
    Mixtral,
    #[default]
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

pub async fn completion(
    api_key: String,
    model: String,
    system_message: String,
    user_message: String,
) -> Result<Response, anyhow::Error> {
    openai::compat::completion(GROQ_CHAT_API, api_key, model, system_message, user_message).await
}
