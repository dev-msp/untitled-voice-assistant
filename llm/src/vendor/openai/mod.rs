pub mod compat;

use self::compat::Response;

const OPENAI_CHAT_API: &str = "https://api.openai.com/v1/";
pub async fn completion(
    api_key: String,
    model: String,
    system_message: String,
    user_message: String,
) -> Result<Response, anyhow::Error> {
    compat::completion(
        OPENAI_CHAT_API,
        api_key,
        model,
        system_message,
        user_message,
    )
    .await
}

pub async fn list_models(api_key: String) -> anyhow::Result<Vec<String>> {
    compat::list_models(OPENAI_CHAT_API, api_key).await
}
