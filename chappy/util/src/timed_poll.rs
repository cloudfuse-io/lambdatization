use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use tracing::{debug, error, warn};

pub struct TimedPoll<F> {
    future: F,
    tag: &'static str,
}

pub fn timed_poll<F: Future>(tag: &'static str, future: F) -> TimedPoll<F> {
    TimedPoll { future, tag }
}

impl<F: Future> Future for TimedPoll<F> {
    type Output = F::Output;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let before = std::time::Instant::now();
        let tag = self.tag;
        let future = unsafe { Pin::map_unchecked_mut(self, |me| &mut me.future) };
        let res = future.poll(cx);
        let poll_duration = before.elapsed();
        if poll_duration > Duration::from_secs(1) {
            debug!(elapsed = ?poll_duration, tag, "slow poll");
        }
        res
    }
}

pub fn timed_drop<F: Sized>(tag: &'static str, obj: F) {
    let handle = tokio::task::spawn(async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        warn!(tag, "slow drop 10ms");
        tokio::time::sleep(Duration::from_secs(1)).await;
        error!(tag, "slow drop 1s");
    });
    drop(obj);
    handle.abort();
}
