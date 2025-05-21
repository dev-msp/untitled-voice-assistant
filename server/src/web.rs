use actix_multipart::Multipart; // Import for multipart handling
use actix_web::{
    body::BoxBody,
    get,
    http::header::ContentType,
    middleware::Logger,
    post,
    web::{self, Data},
    App, HttpRequest, HttpResponse, HttpServer, Responder,
};
use bytes::BytesMut; // For collecting multipart data
use crossbeam::channel::{Receiver, Sender};
use futures_util::stream::StreamExt as _; // For processing multipart stream
use serde::Serialize;
use std::{fs, io};
use tokio::{spawn, task::JoinHandle};
use voice::{
    app::{
        command::{Plumbing, TranscriptionParams},
        response::Response,
        state::Mode,
    }, // Import TranscriptionParams
    audio::Session,
    // OPERATIVE NOTE:
    // Assume Plumbing::Transcribe variant and required structs like TranscriptionParams
    // and associated response handling are defined elsewhere (e.g., in voice::app::command)
    // voice::app::command::TranscriptionParams, // Already imported above
};

struct ApiResponder<T> {
    content: T,
}

impl<T: Serialize> Responder for ApiResponder<T> {
    type Body = BoxBody;

    fn respond_to(self, _req: &HttpRequest) -> HttpResponse {
        HttpResponse::Ok().json(self.content)
    }
}
use std::marker::Send;

// Represents anything that can send commands (In) and receive responses (Out).
// This captures the core pattern used to communicate with the application backend.
trait EventLoop<In, Out>
where
    In: Send + 'static, // Commands must be sendable across threads
    Out: Send + 'static,
{
    type Error: std::error::Error;

    fn start(self) -> JoinHandle<Result<(), Self::Error>>;

    // Note: specific command methods like `start`, `stop`, `mode`
    // are specific implementations built on top of this general request pattern.
    // They belong on the implementing struct (like AppEvents), not the trait.
}

struct AppEvents<In, Out>(Sender<In>, Receiver<Out>);

type AppChannel<In, Out> = web::Data<AppEvents<In, Out>>;

impl AppEvents<Plumbing, Response> {
    fn start(&self, session: Session) -> Response {
        self.request(Plumbing::Start(session))
    }

    fn stop(&self) -> Response {
        self.request(Plumbing::Stop)
    }

    fn mode(&self, mode: Mode) -> Response {
        self.request(Plumbing::Mode(mode))
    }

    // Add the transcribe command sender
    fn transcribe(&self, audio_data: Vec<u8>, params: TranscriptionParams) -> Response {
        self.request(Plumbing::Transcribe { audio_data, params })
    }

    fn request(&self, cmd: Plumbing) -> Response {
        self.0.send(cmd).unwrap(); // In a real app, handle send errors
        self.1.recv().unwrap() // In a real app, handle recv errors
    }
}

#[post("/start")]
async fn start(app: AppChannel<Plumbing, Response>, session: web::Json<Session>) -> impl Responder {
    let response = app.start(session.into_inner());
    ApiResponder { content: response }
}

#[post("/stop")]
async fn stop(app: AppChannel<Plumbing, Response>) -> impl Responder {
    let response = app.stop();
    ApiResponder { content: response }
}

#[post("/mode")]
async fn set_mode(app: AppChannel<Plumbing, Response>, mode: web::Json<Mode>) -> impl Responder {
    let response = app.mode(mode.into_inner());
    ApiResponder { content: response }
}

// ApiCommand/ApiResponse and associated impls/structs removed based on inline notes
// suggesting a single command/response channel to the daemon.
// fn transcribe handler updated to use AppChannel.

<<<<<<< Conflict 1 of 3
+++++++ Contents of side #1
// #[derive(Debug, serde::Serialize, serde::Deserialize)] // Assuming this struct is still relevant for the request body
struct TranscribeRequest {
    content: String, // This probably needs to be adjusted for multipart file upload
    // Add fields for transcription parameters based on audio.md point 6
    model: Option<voice::whisper::transcription::Model>,
    sample_rate: Option<u32>,
    prompt: Option<String>,
}

%%%%%%% Changes from base to side #2
-// #[derive(Debug, serde::Serialize, serde::Deserialize)] // Assuming this struct is still relevant for the request body
-// struct TranscribeRequest {
-//     content: String, // This probably needs to be adjusted for multipart file upload
-//     // Add fields for transcription parameters based on audio.md point 6
-//     // model: Option<voice::whisper::transcription::Model>,
-//     // sample_rate: Option<u32>,
-//     // prompt: Option<String>,
-// }
-
+// Update transcribe handler to accept multipart and use AppChannel
>>>>>>> Conflict 1 of 3 ends
#[post("/transcribe")]
<<<<<<< Conflict 2 of 3
%%%%%%% Changes from base to side #1
 // Signature changed to use AppChannel
 // The body would need significant changes to handle multipart and call app.transcribe
-async fn transcribe(_app: AppChannel, _req: web::Json<serde_json::Value>) -> impl Responder {
+async fn transcribe(
+    _app: AppChannel<Plumbing, Response>,
+    _req: web::Json<serde_json::Value>,
+) -> impl Responder {
     // Using Value as a placeholder for the complex request
     // let audio_data = ... extract from multipart request ...
     // let params = ... extract params from multipart request ...
     // let response = app.transcribe(audio_data, params); // Requires app.transcribe and Plumbing::Transcribe
     // ApiResponder { content: response } // Assuming Response::Transcription is used
     log::warn!("transcribe endpoint is a placeholder and needs multipart implementation");
     HttpResponse::NotImplemented().finish() // Return a placeholder response
+++++++ Contents of side #2
async fn transcribe(app: AppChannel, mut payload: Multipart) -> impl Responder {
    let mut audio_data: Option<Vec<u8>> = None;
    let mut params: TranscriptionParams = TranscriptionParams {
        model: None,
        sample_rate: None,
        prompt: None,
    };

    // Process multipart fields
    while let Some(mut field) = payload.next().await {
        let field_name = field.name().to_string();
        log::debug!("Received multipart field: {}", field_name);

        let mut bytes = BytesMut::new();
        while let Some(chunk) = field.next().await {
            match chunk {
                Ok(chunk) => bytes.extend_from_slice(&chunk),
                Err(e) => {
                    log::error!("Error reading multipart chunk: {}", e);
                    return HttpResponse::InternalServerError().finish();
                }
            }
        }
        let data = bytes.freeze();

        match field_name.as_str() {
            "audio" => {
                audio_data = Some(data.to_vec());
            }
            "model" => {
                // Assuming model is sent as a string like "small"
                let model_str = String::from_utf8_lossy(&data);
                match voice::whisper::transcription::Model::from_str(&model_str) {
                    Ok(model) => params.model = Some(model),
                    Err(e) => {
                        log::warn!("Failed to parse model '{}': {}", model_str, e);
                        // Optionally return an error response or ignore
                    }
                }
            }
            "sample_rate" => {
                // Assuming sample_rate is sent as a number string
                let sr_str = String::from_utf8_lossy(&data);
                match sr_str.parse::<u32>() {
                    Ok(sr) => params.sample_rate = Some(sr),
                    Err(e) => {
                        log::warn!("Failed to parse sample_rate '{}': {}", sr_str, e);
                        // Optionally return an error response or ignore
                    }
                }
            }
            "prompt" => {
                // Assuming prompt is sent as text
                params.prompt = Some(String::from_utf8_lossy(&data).into_owned());
            }
            _ => {
                log::warn!("Ignoring unknown multipart field: {}", field_name);
            }
        }
    }

    let Some(audio) = audio_data else {
        log::error!("No audio data received in transcribe request");
        return HttpResponse::BadRequest().body("Missing audio data");
    };

    // Send the transcribe command to the daemon
    let response = app.transcribe(audio, params);

    // Respond with the daemon's response
    ApiResponder { content: response }
>>>>>>> Conflict 2 of 3 ends
}

#[get("/")]
async fn serve_index_page() -> impl Responder {
    match fs::read_to_string("../server/templates/index.html") {
        Ok(content) => HttpResponse::Ok()
            .content_type(ContentType::html())
            .body(content),
        Err(e) => {
            log::error!("Failed to read index.html: {}", e);
            HttpResponse::NotFound().finish()
        }
    }
}

#[derive(Debug, Clone)]
pub struct Server<In, Out> {
    addr: (String, u16),
    input: Sender<In>,
    output: Receiver<Out>,
}

impl<In, Out> EventLoop<In, Out> for Server<In, Out>
where
    In: std::marker::Send + Clone + 'static,
    Out: std::marker::Send + Clone + 'static,
{
    type Error = std::io::Error;

    fn start(self) -> JoinHandle<Result<(), Self::Error>> {
        spawn(async move {
            let addr = self.addr.clone();
            let server = HttpServer::new(move || {
                let scope = self.voice_scope();
                App::new()
                    .wrap(Logger::default())
                    .service(serve_index_page)
                    .service(scope)
            })
            .bind(&addr)?;

            let handle = server.run().await;
            log::warn!("Server finished?");
            handle
        })
    }
}

impl<In, Out> Server<In, Out>
where
    In: std::marker::Send + Clone + 'static,
    Out: std::marker::Send + Clone + 'static,
{
    #[must_use]
    pub fn new(addr: (String, u16), input: Sender<In>, output: Receiver<Out>) -> Self {
        Self {
            addr,
<<<<<<< Conflict 3 of 3
+++++++ Contents of side #1
            input,
            output,
        }
    }

    fn voice_scope(&self) -> actix_web::Scope {
        web::scope("/voice")
            .service(start)
            .service(stop)
            .service(set_mode)
            // .service(transcribe) // Add transcribe here once implemented
            .app_data(Data::new(AppEvents(
                self.input.clone(),
                self.output.clone(),
            )))
    }

    pub async fn run(self) -> io::Result<()> {
        if let Err(e) = self.start().await {
            log::error!("Server error: {e}");
            Err(e.into())
        } else {
            Ok(())
        }
%%%%%%% Changes from base to side #2
             commands,
             responses,
             // apiCommands: apiCommands, // removed
             // apiResponses: apiResponses, // removed
         }
     }
 
     pub async fn run(self) -> std::io::Result<()> {
         let server = HttpServer::new(move || {
             // Clone the channels for each worker
             let commands_clone = self.commands.clone();
             let responses_clone = self.responses.clone();
 
             let voice = web::scope("/voice")
                 .service(start)
                 .service(stop)
                 .service(set_mode)
-                // .service(transcribe) // Add transcribe here once implemented
+                .service(transcribe) // Add transcribe here once implemented
                 .app_data(Data::new(AppEvents(
                     commands_clone,  // Use cloned channels
                     responses_clone, // Use cloned channels
                 )));
 
             App::new()
                 .wrap(Logger::default())
                 .service(serve_index_page)
                 .service(voice)
         })
         .bind(&self.addr)?;
 
         let handle = server.run().await;
         log::warn!("Server finished?");
         handle
>>>>>>> Conflict 3 of 3 ends
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AddressParseError {
    #[error("missing host")]
    MissingHost,

    #[error("missing port")]
    MissingPort,

    #[error("parse error: {0}")]
    ParsePort(#[from] std::num::ParseIntError),
}

pub fn parse_addr_option(s: &str) -> Result<(String, u16), AddressParseError> {
    let mut parts = s.split(':');
    let host = parts.next().ok_or(AddressParseError::MissingHost)?;
    let port = parts
        .next()
        .ok_or(AddressParseError::MissingPort)?
        .parse()
        .map_err(AddressParseError::ParsePort)?;

    Ok((host.to_string(), port))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{http::header::ContentType, test, App};
    // Mocking channel types for tests if needed, or just test endpoints that don't rely on channels
    use crossbeam::channel; // Need channels for mocking AppChannel

    // Helper to create a mock AppChannel for testing handlers
    fn create_mock_app_channel() -> AppChannel {
        let (cmd_tx, _cmd_rx) = channel::unbounded(); // Mute receiver if not used
        let (_resp_tx, resp_rx) = channel::unbounded(); // Mute sender if not used
        Data::new(AppEvents(cmd_tx, resp_rx))
    }

    #[actix_web::test]
    async fn test_serve_index_page_success() {
        // This test doesn't need the mock AppChannel
        let app = test::init_service(App::new().service(serve_index_page)).await;

        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

        let content_type = resp
            .headers()
            .get(actix_web::http::header::CONTENT_TYPE)
            .expect("Response should have a content type");

        assert_eq!(
            content_type.to_str().unwrap().to_owned(),
            ContentType::html().to_string(),
            "Content-Type should be text/html"
        );

        let body_bytes = test::read_body(resp).await;
        assert!(String::from_utf8(body_bytes.to_vec()).is_ok());
    }

    // Note: Other tests for start, stop, mode, transcribe endpoints would require
    // mocking the AppChannel or setting up a test daemon, which is beyond the
    // scope of this specific refactor based on inline notes.
    // The test_transcribe_placeholder_response above shows how to provide a mock channel.
    // A full test would involve setting up channels and having mock sender/receiver logic
    // to simulate the daemon's response.
}
