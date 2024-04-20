use crate::app::{response::Response, state::Mode};

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    Start {
        #[clap(short, long)]
        input_device: Option<String>,

        #[clap(long)]
        sample_rate: Option<u32>,
    },
    Stop,
    Reset,
    ChangeMode {
        #[arg(value_enum)]
        mode: Mode,
    },
}

#[derive(Debug, clap::Args)]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,

    host: String,
}

// make struct "running app" that has command and client fields, and implements From<App>

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
    pub async fn execute(self) -> Result<Response, anyhow::Error> {
        let resp = match self.command {
            Commands::Start {
                input_device,
                sample_rate,
            } => self.client.start(input_device, sample_rate).await,
            Commands::Stop => self.client.stop().await,
            Commands::Reset => self.client.reset().await,
            Commands::ChangeMode { mode } => self.client.change_mode(mode).await,
        }?;
        Ok(serde_json::from_str(
            &resp.error_for_status()?.text().await?,
        )?)
    }
}

mod api {
    use crate::{app::state::Mode, audio::Session};

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
        ) -> Result<reqwest::Response, anyhow::Error> {
            let body =
                serde_json::to_value(Session::new(input_device, sample_rate, None))?.to_string();
            println!("body: {body}");
            let req = self.post("/voice/start").body(body).build()?;
            Ok(self.execute(req).await?)
        }

        pub async fn stop(&self) -> Result<reqwest::Response, anyhow::Error> {
            let req = self.post("/voice/stop").build()?;
            Ok(self.execute(req).await?)
        }

        pub async fn reset(&self) -> Result<reqwest::Response, anyhow::Error> {
            let req = self.post("/voice/reset").build()?;
            Ok(self.execute(req).await?)
        }

        pub async fn change_mode(&self, mode: Mode) -> Result<reqwest::Response, anyhow::Error> {
            let req = self
                .post("/voice/mode")
                .body(serde_json::json!({ "mode": mode }).to_string())
                .build()?;
            Ok(self.execute(req).await?)
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
        fn execute(
            &self,
            request: reqwest::Request,
        ) -> impl std::future::Future<Output = Result<reqwest::Response, reqwest::Error>> {
            self.inner.execute(request)
        }
    }
}
