use serde::{Deserialize, Serialize};

use super::command::Plumbing;
use crate::audio::Session;

#[derive(Debug, Default, Clone, PartialEq, Deserialize, Serialize)]
pub enum Audio {
    #[default]
    Idle,
    Started(Session),
    Stopped(Session),
}

pub trait State {
    type Input;
    type Output;

    fn handle(&mut self, input: Self::Input) -> Self::Output;
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct RecordingState {
    audio: Audio,
    mode: Mode,
}

impl State for RecordingState {
    type Input = Plumbing;
    type Output = bool;

    fn handle(&mut self, cmd: Self::Input) -> Self::Output {
        match cmd {
            Plumbing::Start(session) => self.start(session.clone()),
            Plumbing::Stop => self.stop(),
            Plumbing::Mode(mode) if !self.running() => self.change_mode(mode.clone()),
            Plumbing::Mode(_) => false,
            // Nothing changes about the state when we send these commands, but we still need to
            // return true so the event loop is triggered.
            //
            // TODO: I should consider making the event loop not sort of dependent on changes in
            // the state and find some other way to represent that.
            Plumbing::Reset | Plumbing::Respond(_) => true,
        }
    }
}

impl RecordingState {
    #[must_use]
    pub fn running(&self) -> bool {
        matches!(self.audio, Audio::Started(_))
    }

    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode.clone()
    }

    #[must_use]
    pub fn session(&self) -> Option<&Session> {
        match &self.audio {
            Audio::Started(s) | Audio::Stopped(s) => Some(s),
            Audio::Idle => None,
        }
    }

    pub fn prompt(&self) -> Option<String> {
        match &self.audio {
            Audio::Started(s) | Audio::Stopped(s) => s.prompt().map(str::to_owned),
            Audio::Idle => None,
        }
    }

    fn start(&mut self, session: Session) -> bool {
        match self.audio {
            Audio::Idle | Audio::Stopped(_) => {
                self.audio = Audio::Started(session);
                true
            }
            Audio::Started(_) => false,
        }
    }

    fn stop(&mut self) -> bool {
        match &self.audio {
            Audio::Started(s) => {
                self.audio = Audio::Stopped(s.clone());
                true
            }
            _ => false,
        }
    }

    pub fn change_mode(&mut self, mode: Mode) -> bool {
        if self.mode == mode {
            false
        } else {
            self.mode = mode;
            true
        }
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, clap::ValueEnum, Deserialize, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum Mode {
    #[serde(rename = "standard")]
    Standard,

    #[default]
    #[serde(rename = "live_typing")]
    LiveTyping,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Standard => write!(f, "standard"),
            Self::LiveTyping => write!(f, "live_typing"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whisper::transcription::Model;

    fn create_dummy_session() -> Session {
        Session::new(
            Some("dummy_device".to_string()),
            Some(44100),
            Some("dummy prompt".to_string()),
            Some(Model::Small),
        )
    }

    #[test]
    fn test_default_state() {
        let state = RecordingState::default();
        assert_eq!(state.audio, Audio::Idle);
        assert_eq!(state.mode, Mode::LiveTyping);
        assert!(!state.running());
        assert_eq!(state.session(), None);
        assert_eq!(state.prompt(), None);
    }

    #[test]
    fn test_start_from_idle() {
        let mut state = RecordingState::default();
        let session = create_dummy_session();
        let started = state.start(session.clone());
        assert!(started);
        assert_eq!(state.audio, Audio::Started(session));
        assert!(state.running());
        assert!(state.session().is_some());
        assert!(state.prompt().is_some());
    }

    #[test]
    fn test_start_from_stopped() {
        let mut state = RecordingState::default();
        let session1 = create_dummy_session();
        state.audio = Audio::Stopped(session1.clone());
        let session2 = create_dummy_session(); // New session for starting
        let started = state.start(session2.clone());
        assert!(started);
        assert_eq!(state.audio, Audio::Started(session2));
        assert!(state.running());
        assert!(state.session().is_some());
        assert!(state.prompt().is_some());
    }

    #[test]
    fn test_start_from_started() {
        let mut state = RecordingState::default();
        let session1 = create_dummy_session();
        state.audio = Audio::Started(session1.clone());
        let session2 = create_dummy_session(); // Attempt to start with a new session
        let started = state.start(session2);
        assert!(!started); // Should fail to start when already started
        assert_eq!(state.audio, Audio::Started(session1)); // State should remain started with the original session
        assert!(state.running());
    }

    #[test]
    fn test_stop_from_started() {
        let mut state = RecordingState::default();
        let session = create_dummy_session();
        state.audio = Audio::Started(session.clone());
        let stopped = state.stop();
        assert!(stopped);
        assert_eq!(state.audio, Audio::Stopped(session));
        assert!(!state.running());
        assert!(state.session().is_some()); // Should still have the session reference in Stopped state
    }

    #[test]
    fn test_stop_from_idle_or_stopped() {
        let mut state_idle = RecordingState::default();
        let stopped_idle = state_idle.stop();
        assert!(!stopped_idle);
        assert_eq!(state_idle.audio, Audio::Idle);

        let mut state_stopped = RecordingState::default();
        let session = create_dummy_session();
        state_stopped.audio = Audio::Stopped(session.clone());
        let stopped_stopped = state_stopped.stop();
        assert!(!stopped_stopped);
        assert_eq!(state_stopped.audio, Audio::Stopped(session));
    }

    #[test]
    fn test_running() {
        let mut state = RecordingState::default();
        assert!(!state.running());

        let session = create_dummy_session();
        state.audio = Audio::Started(session.clone());
        assert!(state.running());

        state.audio = Audio::Stopped(session);
        assert!(!state.running());
    }

    #[test]
    fn test_session() {
        let mut state = RecordingState::default();
        assert_eq!(state.session(), None);

        let session_started = create_dummy_session();
        state.audio = Audio::Started(session_started.clone());
        assert_eq!(state.session(), Some(&session_started));

        let session_stopped = create_dummy_session();
        state.audio = Audio::Stopped(session_stopped.clone());
        assert_eq!(state.session(), Some(&session_stopped));
    }

    #[test]
    fn test_prompt() {
        let mut state = RecordingState::default();
        assert_eq!(state.prompt(), None);

        let session_with_prompt = create_dummy_session();
        state.audio = Audio::Started(session_with_prompt.clone());
        assert_eq!(state.prompt(), Some("dummy prompt".to_string()));

        let session_without_prompt = Session::new(None, None, None, None);
        state.audio = Audio::Started(session_without_prompt.clone());
        assert_eq!(state.prompt(), None);

        let session_stopped_with_prompt = create_dummy_session();
        state.audio = Audio::Stopped(session_stopped_with_prompt.clone());
        assert_eq!(state.prompt(), Some("dummy prompt".to_string()));

        let session_stopped_without_prompt = Session::new(None, None, None, None);
        state.audio = Audio::Stopped(session_stopped_without_prompt.clone());
        assert_eq!(state.prompt(), None);
    }

    #[test]
    fn test_change_mode() {
        let mut state = RecordingState::default(); // Default is LiveTyping
        assert_eq!(state.mode(), Mode::LiveTyping);

        // Change to Standard
        let changed_to_standard = state.change_mode(Mode::Standard);
        assert!(changed_to_standard);
        assert_eq!(state.mode(), Mode::Standard);

        // Change back to LiveTyping
        let changed_to_livetyping = state.change_mode(Mode::LiveTyping);
        assert!(changed_to_livetyping);
        assert_eq!(state.mode(), Mode::LiveTyping);

        // Change to the same mode (LiveTyping)
        let changed_to_same = state.change_mode(Mode::LiveTyping);
        assert!(!changed_to_same); // Should return false
        assert_eq!(state.mode(), Mode::LiveTyping);
    }

    #[test]
    fn test_next_state_start() {
        let mut state = RecordingState::default(); // Idle
        let session = create_dummy_session();
        let command = Plumbing::Start(session.clone());
        let changed = state.handle(command.clone());
        assert!(changed);
        assert_eq!(state.audio, Audio::Started(session.clone()));

        // Already Started
        let changed_again = state.handle(command);
        assert!(!changed_again); // Should not change state
        assert_eq!(state.audio, Audio::Started(session));
    }

    #[test]
    fn test_next_state_stop() {
        let mut state = RecordingState::default(); // Idle
        let command = Plumbing::Stop;
        let changed = state.handle(command.clone());
        assert!(!changed); // Cannot stop from Idle
        assert_eq!(state.audio, Audio::Idle);

        let session = create_dummy_session();
        state.audio = Audio::Started(session.clone()); // Started
        let changed_from_started = state.handle(command.clone());
        assert!(changed_from_started);
        assert_eq!(state.audio, Audio::Stopped(session.clone()));

        // Already Stopped
        let changed_from_stopped = state.handle(command);
        assert!(!changed_from_stopped); // Cannot stop from Stopped
        assert_eq!(state.audio, Audio::Stopped(session));
    }

    #[test]
    fn test_next_state_mode() {
        let mut state = RecordingState::default(); // LiveTyping, not running
        let command_standard = Plumbing::Mode(Mode::Standard);
        let changed_to_standard = state.handle(command_standard.clone());
        assert!(changed_to_standard);
        assert_eq!(state.mode(), Mode::Standard);

        let command_livetyping = Plumbing::Mode(Mode::LiveTyping);
        let changed_to_livetyping = state.handle(command_livetyping);
        assert!(changed_to_livetyping);
        assert_eq!(state.mode(), Mode::LiveTyping);

        let session = create_dummy_session();
        state.audio = Audio::Started(session); // Running
        let changed_while_running = state.handle(command_standard);
        assert!(!changed_while_running); // Cannot change mode while running
        assert_eq!(state.mode(), Mode::LiveTyping); // Mode should not have changed
    }

    #[test]
    fn test_next_state_reset_and_respond() {
        let mut state = RecordingState::default(); // Idle
        let command_reset = Plumbing::Reset;
        let changed_reset = state.handle(command_reset);
        assert!(changed_reset); // Should return true to trigger event loop

        let command_respond = Plumbing::Respond(crate::app::response::Response::Nil);
        let changed_respond = state.handle(command_respond);
        assert!(changed_respond); // Should return true to trigger event loop

        // State should not have actually changed for Reset/Respond commands
        assert_eq!(state.audio, Audio::Idle);
        assert_eq!(state.mode, Mode::LiveTyping);
    }
}
