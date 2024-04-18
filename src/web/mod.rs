use actix_web::{
    body::BoxBody,
    middleware::Logger,
    post,
    web::{self, Data},
    App, HttpRequest, HttpResponse, HttpServer, Responder,
};
use crossbeam::channel::{Receiver, Sender};
use serde::Serialize;

use crate::app::{
    command::Command,
    response::Response,
    state::{Mode, RecordingSession},
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

type AppChannel = web::Data<AppEvents>;

struct AppEvents(Sender<Command>, Receiver<Response>);

impl AppEvents {
    fn start(&self, session: RecordingSession) -> Response {
        self.request(Command::Start(session))
    }

    fn stop(&self) -> Response {
        self.request(Command::Stop)
    }

    fn mode(&self, mode: Mode) -> Response {
        self.request(Command::Mode(mode))
    }

    fn request(&self, cmd: Command) -> Response {
        self.0.send(cmd).unwrap();
        self.1.recv().unwrap()
    }
}

#[post("/start")]
async fn start(app: AppChannel, session: web::Json<RecordingSession>) -> impl Responder {
    let response = app.start(session.into_inner());
    ApiResponder { content: response }
}

#[post("/stop")]
async fn stop(app: AppChannel) -> impl Responder {
    let response = app.stop();
    ApiResponder { content: response }
}

#[post("/mode")]
async fn set_mode(app: AppChannel, mode: web::Json<Mode>) -> impl Responder {
    let response = app.mode(mode.into_inner());
    ApiResponder { content: response }
}

pub async fn run(commands: Sender<Command>, responses: Receiver<Response>) -> std::io::Result<()> {
    let server = HttpServer::new(move || {
        let voice = web::scope("/voice")
            .service(start)
            .service(stop)
            .service(set_mode)
            .app_data(Data::new(AppEvents(commands.clone(), responses.clone())));

        App::new().wrap(Logger::default()).service(voice)
    })
    .bind(("localhost", 8088))?;

    let handle = server.run().await;
    log::warn!("Server finished?");
    handle
}
