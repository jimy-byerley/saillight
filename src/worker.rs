//! background work (scan, compression) on its own thread, cancellation, and the bridge back
//! to the Qt thread

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

/// name of the threads running background work
const THREAD_NAME: &str = "saillight-worker";

/// asks a running task to stop. the task checks it between two steps of its work
#[derive(Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// run `task` on a new thread, then give its result to `deliver` from that thread
pub fn spawn_then<T, F, D>(task: F, deliver: D) -> Cancel
where
    T: Send + 'static,
    F: FnOnce(&Cancel) -> T + Send + 'static,
    D: FnOnce(T) + Send + 'static,
{
    let cancel = Cancel::default();
    let given = cancel.clone();
    thread::Builder::new()
        .name(THREAD_NAME.to_owned())
        .spawn(move || deliver(task(&given)))
        .expect("failed to start a worker thread");
    cancel
}

/// run `task` on a new thread, then give its result to `callback` on the Qt thread.
///
/// must be called from the Qt thread. a cancelled task still calls back, with whatever
/// it returns when it notices the cancellation.
#[allow(dead_code)] // first used by the scan
pub fn spawn_to_qt<T, F, C>(task: F, callback: C) -> Cancel
where
    T: Send + 'static,
    F: FnOnce(&Cancel) -> T + Send + 'static,
    C: FnOnce(T) + 'static,
{
    let mut callback = Some(callback);
    let queued = qmetaobject::queued_callback(move |result: T| {
        if let Some(callback) = callback.take() {
            callback(result);
        }
    });
    spawn_then(task, move |result| queued(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    const TIMEOUT: Duration = Duration::from_secs(5);

    #[test]
    fn result_reaches_the_callback() {
        let (sender, receiver) = mpsc::channel();
        spawn_then(|_| 6 * 7, move |result| sender.send(result).unwrap());
        assert_eq!(receiver.recv_timeout(TIMEOUT), Ok(42));
    }

    #[test]
    fn task_runs_on_a_worker_thread() {
        let (sender, receiver) = mpsc::channel();
        spawn_then(
            |_| thread::current().name().map(str::to_owned),
            move |name| sender.send(name).unwrap(),
        );
        let name = receiver.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(name.as_deref(), Some(THREAD_NAME));
    }

    #[test]
    fn task_sees_the_cancellation() {
        let (started_sender, started) = mpsc::channel();
        let (sender, receiver) = mpsc::channel();
        let cancel = spawn_then(
            move |cancel| {
                started_sender.send(()).unwrap();
                let mut steps = 0u32;
                while !cancel.is_cancelled() && steps < 5000 {
                    thread::sleep(Duration::from_millis(1));
                    steps += 1;
                }
                cancel.is_cancelled()
            },
            move |cancelled| sender.send(cancelled).unwrap(),
        );
        started.recv_timeout(TIMEOUT).unwrap();
        assert!(!cancel.is_cancelled());
        cancel.cancel();
        assert_eq!(receiver.recv_timeout(TIMEOUT), Ok(true));
    }
}
