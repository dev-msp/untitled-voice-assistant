use std::{
    fmt::Debug,
    sync::{Arc, Condvar, Mutex},
};

#[derive(Debug, Clone)]
pub struct Notifier<T: Clone>(Arc<(Mutex<T>, Condvar)>);

impl<T: Debug + Clone + PartialEq> Notifier<T> {
    pub fn notify(&self, value: T) {
        let (lock, cvar) = &*self.0;
        log::trace!("Trying to lock (notify)");
        let mut state = lock.lock().unwrap();
        log::trace!("Locked (notify)");
        *state = value;
        cvar.notify_one();
    }

    pub fn wait_until(&self, value: &T) {
        let (lock, cvar) = &*self.0;
        log::trace!("Trying to lock (wait_until)");
        let mut state = lock.lock().unwrap();
        log::trace!("Locked (wait_until)");
        log::trace!("Waiting for value: {:?}", value);
        while *state != *value {
            state = cvar.wait(state).unwrap();
            log::trace!("got value: {:?}", &*state);
            if *state != *value {
                log::trace!("Got wrong value ({:?}), continuing", &*state);
            }
        }
    }
}

impl<T: Clone + Default> Notifier<T> {
    pub fn new() -> Self {
        Self(Arc::new((Mutex::new(T::default()), Condvar::new())))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RecordState {
    #[default]
    Stopped,
    Started,
    Recording,
}

#[derive(Debug, Clone)]
pub struct Controller {
    notifier: Notifier<RecordState>,
}

impl Controller {
    pub fn new() -> Self {
        Self {
            notifier: Notifier::new(),
        }
    }

    pub fn start(&self) {
        self.notifier.notify(RecordState::Started);
    }

    pub fn recording(&self) {
        self.notifier.notify(RecordState::Recording);
    }

    pub fn stop(&self) {
        self.notifier.notify(RecordState::Stopped);
    }

    pub fn wait_for(&self, state: RecordState) {
        self.notifier.wait_until(&state);
    }
}
