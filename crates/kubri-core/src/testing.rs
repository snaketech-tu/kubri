//! A fake API server for unit tests.

use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use http::{Request, Response, StatusCode};
use kube::Client;
use kube::client::Body;

/// A request the fake API server received.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    /// Path and query.
    pub uri: String,
}

/// A client whose requests are answered by `respond` with a status and a body.
pub fn client<F>(respond: F) -> (Client, Arc<Mutex<Vec<Recorded>>>)
where
    F: Fn(&Recorded) -> (u16, String) + Send + Sync + 'static,
{
    let log = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&log);
    let respond = Arc::new(respond);
    let service = tower::service_fn(move |req: Request<Body>| {
        let recorded = Recorded {
            method: req.method().to_string(),
            uri: req.uri().to_string(),
        };
        let (status, body) = respond(&recorded);
        recorder.lock().unwrap().push(recorded);
        async move {
            let mut response = Response::new(Body::from(body.into_bytes()));
            *response.status_mut() = StatusCode::from_u16(status).unwrap();
            Ok::<_, Infallible>(response)
        }
    });
    (Client::new(service, "default"), log)
}

/// The JSON of a failed API call.
pub fn status(code: u16, message: &str) -> String {
    serde_json::json!({
        "kind": "Status", "apiVersion": "v1", "metadata": {},
        "status": "Failure", "message": message, "reason": "", "code": code
    })
    .to_string()
}
