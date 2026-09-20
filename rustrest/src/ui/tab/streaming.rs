//! Generic bridge from a tokio background task that reports events through
//! an `mpsc` channel to an `iced::Task` that dispatches one `Message` per event.

use tokio::sync::mpsc::Sender;

/// Caps how many events can be queued between the producer (the network
/// task) and the consumer (iced's UI update loop) before `Sender::send`
/// starts blocking the producer. Without this bound, a fast feed (e.g. a
/// busy SSE stream) that outpaces UI rendering would queue an ever-growing
/// backlog in memory instead of just pausing the read until the UI catches
/// up - backpressure here, not an unbounded buffer, is what keeps memory
/// flat regardless of how fast the source produces events.
const CHANNEL_CAPACITY: usize = 64;

/// Spawns `session` as a background task, wiring its `Sender<Event>` side
/// to an `iced::Task` that maps each event to a `Message` via `on_event`,
/// then dispatches `on_done` once `session` returns (the channel's sender
/// side was dropped, whether because the session finished normally or the
/// tab cancelled it).
pub fn spawn_streaming<Event, Fut, Message>(
    session: impl FnOnce(Sender<Event>) -> Fut + Send + 'static,
    on_event: impl Fn(Event) -> Message + Send + 'static,
    on_done: impl Fn(()) -> Message + Send + 'static,
) -> iced::Task<Message>
where
    Event: Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
    Message: Send + 'static,
{
    let sipper_task = iced::task::sipper::<Event, _>(move |mut sender| async move {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Event>(CHANNEL_CAPACITY);
        let handle = tokio::spawn(session(tx));

        while let Some(event) = rx.recv().await {
            sender.send(event).await;
        }
        let _ = handle.await;
    });

    iced::Task::sip(sipper_task, on_event, on_done)
}
