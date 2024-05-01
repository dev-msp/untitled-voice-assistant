use serde::{Deserialize, Serialize};

use super::OLLAMA_API;

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

struct Model(String);

#[derive(Debug, Clone, Deserialize)]
pub struct ListModelsResponse {
    models: Vec<LocalModel>,
}

impl From<ListModelsResponse> for Vec<LocalModel> {
    fn from(response: ListModelsResponse) -> Self {
        response.models
    }
}

#[allow(unused)]
#[derive(Debug, Clone, Deserialize)]
pub struct LocalModel {
    name: String,
    size: u64,
    digest: String,
    details: ModelDetails,
}

#[allow(unused)]
#[derive(Debug, Clone, Deserialize)]
struct ModelDetails {
    format: String,
    family: String,
    families: Option<Vec<String>>,
    parameter_size: String,
    quantization_level: String,
}

impl LocalModel {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn human_size(&self) -> String {
        let size = self.size as f64;
        let kilo = 1024.0;
        let mega = kilo * kilo;
        let giga = kilo * mega;
        let tera = kilo * giga;
        let peta = kilo * tera;
        let exa = kilo * peta;
        let zetta = kilo * exa;
        let yotta = kilo * zetta;
        if size < kilo {
            format!("{:.0} B", size)
        } else if size < mega {
            format!("{:.1} KB", size / kilo)
        } else if size < giga {
            format!("{:.1} MB", size / mega)
        } else if size < tera {
            format!("{:.1} GB", size / giga)
        } else if size < peta {
            format!("{:.1} TB", size / tera)
        } else if size < exa {
            format!("{:.1} PB", size / peta)
        } else if size < zetta {
            format!("{:.1} EB", size / exa)
        } else if size < yotta {
            format!("{:.1} ZB", size / zetta)
        } else {
            format!("{:.1} YB", size / yotta)
        }
    }
}

pub async fn list_models() -> anyhow::Result<ListModelsResponse> {
    let resp = reqwest::Client::new()
        .get(format!("{OLLAMA_API}/tags"))
        .send()
        .await?;

    Ok(resp.json().await?)
}
