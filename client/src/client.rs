use voice::{
    app::{response::Response, state::Mode},
    whisper::transcription::Model,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Client error: {0}")]
    Client(#[from] api::Error),
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum Commands {
    Start {
        #[clap(short, long)]
        input_device: Option<String>,

        #[clap(long)]
        sample_rate: Option<u32>,

        #[clap(short, long, value_enum)]
        model: Option<Model>,
    },
    Stop,
    Reset,
    ChangeMode {
        #[arg(value_enum)]
        mode: Mode,
    },
}

#[derive(Debug, clap::Parser)]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,

    host: String,
}

pub struct RunningApp {
    command: Commands,
    client: api::Client,
}

impl From<App> for RunningApp {
    fn from(app: App) -> Self {
        Self {
            command: app.command,
            client: api::Client::new(app.host),
        }
    }
}

impl RunningApp {
    pub async fn execute(self) -> Result<Response, Error> {
        Ok(match self.command {
            Commands::Start {
                input_device,
                sample_rate,
                model,
            } => self.client.start(input_device, sample_rate, model).await,
            Commands::Stop => self.client.stop().await,
            Commands::Reset => self.client.reset().await,
            Commands::ChangeMode { mode } => self.client.change_mode(mode).await,
        }?)
    }
}

pub mod api {
    use serde::de::DeserializeOwned;
    use voice::{
        app::{response::Response, state::Mode},
        audio::Session,
        whisper::transcription::Model,
    };

    #[derive(Debug, thiserror::Error)]
    pub enum Error {
        #[error("HTTP error: {0}")]
        Http(#[from] reqwest::Error),

        #[error("JSON error: {0}")]
        Json(#[from] serde_json::Error),

        #[error("Unexpected response: {0}")]
        UnexpectedResponse(Response),
    }

    pub struct Client {
        inner: reqwest::Client,
        host: String,
    }

    impl Client {
        pub fn new(host: String) -> Self {
            Self {
                inner: reqwest::Client::new(),
                host,
            }
        }

        pub async fn start(
            &self,
            input_device: Option<String>,
            sample_rate: Option<u32>,
            model: Option<Model>,
        ) -> Result<Response, Error> {
            let body = serde_json::to_value(Session::new(input_device, sample_rate, None, model))?
                .to_string();
            println!("body: {body}");
            let req = self.post("/voice/start").body(body).build()?;
            self.execute(req).await
        }

        pub async fn stop(&self) -> Result<Response, Error> {
            let req = self.post("/voice/stop").build()?;
            self.execute(req).await
        }

        pub async fn reset(&self) -> Result<Response, Error> {
            let req = self.post("/voice/reset").build()?;
            self.execute(req).await
        }

        pub async fn change_mode(&self, mode: Mode) -> Result<Response, Error> {
            let req = self
                .post("/voice/mode")
                .body(serde_json::json!({ "mode": mode }).to_string())
                .build()?;
            self.execute(req).await
        }

        fn post(&self, path: &str) -> reqwest::RequestBuilder {
            self.inner
                .post(self.route(path))
                .header("Content-Type", "application/json")
        }

        fn route(&self, path: &str) -> String {
            format!("http://{}{}", self.host, path)
        }

        /// Delegates to the inner client's method.
        async fn execute<T: DeserializeOwned>(
            &self,
            request: reqwest::Request,
        ) -> Result<T, Error> {
            let resp = self.inner.execute(request).await?;
            let r = resp.error_for_status()?;
            Ok(serde_json::from_str(&r.text().await?)?)
        }
    }
}
