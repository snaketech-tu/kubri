use futures::{Stream, StreamExt};
use kube::Api;
use kube::api::DynamicObject;
use kube::runtime::{WatchStreamExt, watcher};
use serde::Serialize;

/// A change to a watched resource list, ready to be sent to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WatchEvent {
    /// The complete current list. Replaces everything the client knows.
    Reset {
        items: Vec<DynamicObject>,
    },
    /// An object was created or modified.
    Upsert {
        item: DynamicObject,
    },
    Delete {
        item: DynamicObject,
    },
    /// The watch failed and is retried with backoff.
    Error {
        message: String,
    },
}

pub(crate) fn watch_objects(api: Api<DynamicObject>) -> impl Stream<Item = WatchEvent> + Send + 'static {
    let mut initial = Vec::new();
    watcher(api, watcher::Config::default())
        .default_backoff()
        .filter_map(move |event| {
            let out = match event {
                Ok(watcher::Event::Init) => {
                    initial.clear();
                    None
                }
                Ok(watcher::Event::InitApply(obj)) => {
                    initial.push(obj);
                    None
                }
                Ok(watcher::Event::InitDone) => Some(WatchEvent::Reset {
                    items: std::mem::take(&mut initial),
                }),
                Ok(watcher::Event::Apply(item)) => Some(WatchEvent::Upsert { item }),
                Ok(watcher::Event::Delete(item)) => Some(WatchEvent::Delete { item }),
                Err(err) => Some(WatchEvent::Error {
                    message: err.to_string(),
                }),
            };
            futures::future::ready(out)
        })
}
