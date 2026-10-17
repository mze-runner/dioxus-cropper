//! Runs a closure on a new thread and awaits its result as a future,
//! using only `std` — no async runtime is assumed.

use super::CropError;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll, Waker};

/// The state shared between the worker thread and the awaiting future.
enum State<T> {
    /// The worker is running; holds the waker of the latest poll.
    Pending(Option<Waker>),
    /// The worker finished; `None` when it unwound without a result.
    Done(Option<T>),
    /// The future has taken the result.
    Taken,
}

type Shared<T> = Arc<Mutex<State<T>>>;

/// Runs `work` on a new thread. The returned future resolves to its
/// result, or to [`CropError::Background`] if the thread cannot be spawned
/// or panics.
pub(super) fn run<T, F>(work: F) -> Completion<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let shared: Shared<T> = Arc::new(Mutex::new(State::Pending(None)));
    let sender = Sender(Arc::clone(&shared));
    let spawned = std::thread::Builder::new()
        .name("dioxus-cropper".into())
        .spawn(move || sender.send(work()));
    match spawned {
        Ok(_) => Completion(Ok(shared)),
        Err(e) => Completion(Err(Some(CropError::Background(Box::new(e))))),
    }
}

/// The worker's end. Dropping it without sending — the worker panicked —
/// completes the future with no result.
struct Sender<T>(Shared<T>);

impl<T> Sender<T> {
    fn send(self, value: T) {
        complete(&self.0, Some(value));
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        complete(&self.0, None);
    }
}

/// Moves a pending state to `Done(value)` and wakes the awaiting task. A
/// state that is already done is left unchanged.
fn complete<T>(shared: &Shared<T>, value: Option<T>) {
    let mut state = shared.lock().unwrap_or_else(PoisonError::into_inner);
    if let State::Pending(waker) = &mut *state {
        let waker = waker.take();
        *state = State::Done(value);
        drop(state);
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

/// The future [`run`] returns. Holds the spawn error instead when the
/// thread could not be started.
pub(super) struct Completion<T>(Result<Shared<T>, Option<CropError>>);

impl<T> Future for Completion<T> {
    type Output = Result<T, CropError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let shared = match &mut self.0 {
            Ok(shared) => shared,
            Err(error) => {
                return Poll::Ready(Err(error.take().unwrap_or_else(|| {
                    CropError::Background("polled after completion".into())
                })))
            }
        };
        let mut state = shared.lock().unwrap_or_else(PoisonError::into_inner);
        match std::mem::replace(&mut *state, State::Taken) {
            State::Pending(_) => {
                *state = State::Pending(Some(cx.waker().clone()));
                Poll::Pending
            }
            State::Done(Some(value)) => Poll::Ready(Ok(value)),
            State::Done(None) => Poll::Ready(Err(CropError::Background(
                "the crop thread panicked".into(),
            ))),
            State::Taken => {
                Poll::Ready(Err(CropError::Background("polled after completion".into())))
            }
        }
    }
}
