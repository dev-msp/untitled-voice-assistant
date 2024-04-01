use std::{
    sync::mpsc,
    thread::{self, JoinHandle},
};

type Reduction<T, R> = Box<dyn Fn(mpsc::Iter<T>) -> R + Send>;

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
        F: Fn(mpsc::Iter<T>) -> R + Send + 'static,
    {
        Self {
            callback: Box::new(callback),
            phantom: std::marker::PhantomData,
        }
    }

    pub fn run(self) -> (mpsc::Sender<T>, JoinHandle<R>) {
        let ProcessNode { callback, .. } = self;

        let (send, recv) = mpsc::channel();

        (
            send,
            thread::spawn(move || {
                let iter = recv.iter();
                callback(iter)
            }),
        )
    }
}
