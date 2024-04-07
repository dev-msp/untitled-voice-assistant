use std::thread::{self, JoinHandle};

use crossbeam::channel::{self, unbounded, Sender};

type Reduction<T, R> = Box<dyn Fn(channel::Iter<T>) -> R + Send>;

pub struct ProcessNode<T, R>
where
    R: Send,
{
    callback: Reduction<T, R>,
    phantom: std::marker::PhantomData<(T, R)>,
}

impl<T, R> ProcessNode<T, R>
where
    T: Send + 'static,
    R: Send + 'static,
{
    pub fn new<F>(callback: F) -> Self
    where
        F: Fn(channel::Iter<T>) -> R + Send + 'static,
    {
        Self {
            callback: Box::new(callback),
            phantom: std::marker::PhantomData,
        }
    }

    pub fn run(self) -> (Sender<T>, JoinHandle<R>) {
        let ProcessNode { callback, .. } = self;

        let (send, recv) = unbounded();

        (
            send,
            thread::spawn(move || {
                let iter = recv.iter();
                callback(iter)
            }),
        )
    }
}
