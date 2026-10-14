// Automation requests: commands that outside tools send to the running game.
//
// Transports (the desktop control server, the web bridge) turn what they receive into Requests
// and queue them with a RequestSender, from any thread. The run loop takes the queued requests
// once per frame on the main thread, runs them, and answers each one through its Responder.
// The commands themselves live in the game (see game::automation).

use std::sync::mpsc;

use serde_json::Value;

#[cfg(feature = "desktop")]
pub mod server;

// ----------------------------------------------
// Request / Response
// ----------------------------------------------

pub struct Request {
    pub method: String,
    pub params: Value,
    pub responder: Responder,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResponseError {
    pub code: String, // E.g. "InvalidParams" or "NotFound".
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Response {
    pub frame: Option<u64>, // Frame the command finished on. None if it never ran.
    pub result: Result<Value, ResponseError>,
}

impl Response {
    // For requests dropped without an answer, e.g. because the game quit first.
    pub fn cancelled() -> Self {
        Self {
            frame: None,
            result: Err(ResponseError {
                code: "Cancelled".into(),
                message: "The game stopped before the command finished.".into(),
            }),
        }
    }
}

// ----------------------------------------------
// Responder
// ----------------------------------------------

// Desktop transports answer from the main thread to their own threads. The web bridge resolves a
// JavaScript promise, which can't leave the (only) thread.
#[cfg(feature = "desktop")]
type RespondFn = Box<dyn FnOnce(Response) + Send>;

#[cfg(not(feature = "desktop"))]
type RespondFn = Box<dyn FnOnce(Response)>;

// Delivers a request's response back to its transport. Every request gets exactly one response:
// a Responder dropped without responding sends Response::cancelled().
pub struct Responder(Option<RespondFn>);

impl Responder {
    #[cfg(feature = "desktop")]
    pub fn new<F>(respond_fn: F) -> Self
    where
        F: FnOnce(Response) + Send + 'static,
    {
        Self(Some(Box::new(respond_fn)))
    }

    #[cfg(not(feature = "desktop"))]
    pub fn new<F>(respond_fn: F) -> Self
    where
        F: FnOnce(Response) + 'static,
    {
        Self(Some(Box::new(respond_fn)))
    }

    pub fn respond(mut self, response: Response) {
        if let Some(respond_fn) = self.0.take() {
            respond_fn(response);
        }
    }
}

impl Drop for Responder {
    fn drop(&mut self) {
        if let Some(respond_fn) = self.0.take() {
            respond_fn(Response::cancelled());
        }
    }
}

// ----------------------------------------------
// RequestQueue
// ----------------------------------------------

#[derive(Clone)]
pub struct RequestSender(mpsc::Sender<Request>);

impl RequestSender {
    // Queues a request for the next frame. If the game has shut down, the request's responder
    // is dropped, which answers it with Response::cancelled().
    pub fn send(&self, request: Request) {
        let _ = self.0.send(request);
    }
}

// Owned by the Engine.
pub(crate) struct RequestQueue {
    sender: mpsc::Sender<Request>,
    receiver: mpsc::Receiver<Request>,
}

impl RequestQueue {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self { sender, receiver }
    }

    #[inline]
    pub fn sender(&self) -> RequestSender {
        RequestSender(self.sender.clone())
    }

    // Requests queued since the last call, oldest first.
    #[inline]
    pub fn take_requests(&self) -> Vec<Request> {
        self.receiver.try_iter().collect()
    }
}

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::json;

    use super::*;

    fn recording_responder() -> (Responder, Arc<Mutex<Vec<Response>>>) {
        let responses = Arc::new(Mutex::new(Vec::new()));
        let sink = responses.clone();
        (Responder::new(move |response| sink.lock().unwrap().push(response)), responses)
    }

    #[test]
    fn test_requests_arrive_in_order_from_other_threads() {
        let queue = RequestQueue::new();
        let sender = queue.sender();

        std::thread::spawn(move || {
            for index in 0..3 {
                let (responder, _) = recording_responder();
                sender.send(Request { method: format!("cmd{index}"), params: json!({}), responder });
            }
        })
        .join()
        .unwrap();

        let methods: Vec<String> = queue.take_requests().into_iter().map(|request| request.method).collect();
        assert_eq!(methods, ["cmd0", "cmd1", "cmd2"]);
        assert!(queue.take_requests().is_empty());
    }

    #[test]
    fn test_responder_responds_once() {
        let (responder, responses) = recording_responder();
        responder.respond(Response { frame: Some(7), result: Ok(json!({"pong": true})) });

        let responses = responses.lock().unwrap();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].frame, Some(7));
        assert_eq!(responses[0].result, Ok(json!({"pong": true})));
    }

    #[test]
    fn test_dropped_responder_cancels() {
        let (responder, responses) = recording_responder();
        drop(responder);

        let responses = responses.lock().unwrap();
        assert_eq!(*responses, [Response::cancelled()]);
    }
}
