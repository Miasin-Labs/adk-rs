use std::pin::Pin;
use std::task::{Context, Poll};

use futures::Stream;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use crate::event::Event;
use crate::ids::{InvocationId, SessionId};
use crate::runner::{RunError, RunOutput, Runner};
use crate::session::SessionStore;

/// An item produced by [`Runner::stream`]: either an event as the run emits it,
/// or the terminal outcome once the run finishes.
#[derive(Debug)]
pub enum RunStreamItem {
    /// An event the run just emitted (user, agent text, tool call/result).
    Event(Event),
    /// The final run outcome; always the last item if the run succeeded.
    Done(RunOutput),
}

/// A stream of [`RunStreamItem`]s for a single run. Events arrive as the run
/// produces them; the last item is `Done(RunOutput)` (or the stream ends early
/// with an error surfaced as the future's `Err`). Borrows the `Runner` for its
/// lifetime `'a`, so it cannot outlive the runner that produced it.
pub struct RunStream<'a> {
    receiver: UnboundedReceiver<Event>,
    run: Pin<Box<dyn std::future::Future<Output = Result<RunOutput, RunError>> + Send + 'a>>,
    finished: Option<Result<RunOutput, RunError>>,
    done_emitted: bool,
}

impl Stream for RunStream<'_> {
    type Item = Result<RunStreamItem, RunError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Terminal: once `Done` has been emitted the stream is exhausted. This
        // must come first so we never re-poll the already-completed run future.
        if self.done_emitted {
            return Poll::Ready(None);
        }
        // Drain any buffered events first so ordering is preserved.
        if let Poll::Ready(Some(event)) = self.receiver.poll_recv(cx) {
            return Poll::Ready(Some(Ok(RunStreamItem::Event(event))));
        }
        // Drive the run forward when no event is buffered.
        if self.finished.is_none() {
            match self.run.as_mut().poll(cx) {
                Poll::Ready(result) => self.finished = Some(result),
                Poll::Pending => return Poll::Pending,
            }
            // The run completed: flush any events it emitted right before finishing.
            if let Poll::Ready(Some(event)) = self.receiver.poll_recv(cx) {
                return Poll::Ready(Some(Ok(RunStreamItem::Event(event))));
            }
        }
        // Run is finished and the channel is drained: emit the terminal item.
        self.done_emitted = true;
        match self.finished.take() {
            Some(Ok(output)) => Poll::Ready(Some(Ok(RunStreamItem::Done(output)))),
            Some(Err(error)) => Poll::Ready(Some(Err(error))),
            None => Poll::Ready(None),
        }
    }
}

impl<S: SessionStore + 'static> Runner<S> {
    /// Run the agent and stream events as they are produced, ending with a
    /// `Done(RunOutput)` item. Equivalent to [`Runner::run`] but incremental.
    pub fn stream(
        &self,
        session_id: &SessionId,
        invocation_id: InvocationId,
        input: impl Into<String>,
    ) -> RunStream<'_> {
        let (sender, receiver) = unbounded_channel();
        let session_id = session_id.clone();
        let input = input.into();
        // SAFETY of lifetimes: the returned future borrows `self`; the stream is
        // tied to that borrow, so it cannot outlive the runner.
        let run = self.run_inner(session_id, invocation_id, input, Some(sender));
        // (`session_id` is owned/moved into the future.)
        RunStream {
            receiver,
            run: Box::pin(run),
            finished: None,
            done_emitted: false,
        }
    }
}
