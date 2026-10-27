//! The desktop app: the shared UI in a native window, backed by kubri-core through Tauri commands.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use kubri_core::{
    ClusterInfo, ContextList, DeleteOptions, EventInfo, Kubri, MetricsKind, ObjectRef, ResourceInfo,
    ResourceRef, TableEvent, Usage,
};
use serde_json::Value;
use tauri::State;
use tauri::async_runtime::JoinHandle;
use tauri::ipc::Channel;
use tracing_subscriber::EnvFilter;

/// Running watches, so the UI can stop them when it navigates away.
#[derive(Default)]
struct Watches {
    next_id: AtomicU32,
    tasks: Mutex<HashMap<u32, JoinHandle<()>>>,
}

type CommandResult<T> = Result<T, String>;

#[tauri::command]
async fn contexts(kubri: State<'_, Arc<Kubri>>) -> CommandResult<ContextList> {
    kubri.contexts().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn reload_contexts(kubri: State<'_, Arc<Kubri>>) -> CommandResult<ContextList> {
    kubri.reload().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn switch_context(kubri: State<'_, Arc<Kubri>>, name: String) -> CommandResult<ClusterInfo> {
    kubri.switch_context(&name).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn resources(kubri: State<'_, Arc<Kubri>>) -> CommandResult<Vec<ResourceInfo>> {
    kubri.resources().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn watch_table(
    kubri: State<'_, Arc<Kubri>>,
    watches: State<'_, Arc<Watches>>,
    resource: ResourceRef,
    namespace: Option<String>,
    on_event: Channel<TableEvent>,
) -> CommandResult<u32> {
    let events = kubri
        .watch_table(&resource, namespace.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    let id = watches.next_id.fetch_add(1, Ordering::Relaxed);
    let registry = Arc::clone(&watches);
    let task = tauri::async_runtime::spawn(async move {
        let mut events = std::pin::pin!(events);
        while let Some(event) = events.next().await {
            if on_event.send(event).is_err() {
                break;
            }
        }
        registry.tasks.lock().unwrap().remove(&id);
    });
    watches.tasks.lock().unwrap().insert(id, task);
    Ok(id)
}

#[tauri::command]
fn unwatch(watches: State<'_, Arc<Watches>>, id: u32) {
    if let Some(task) = watches.tasks.lock().unwrap().remove(&id) {
        task.abort();
    }
}

#[tauri::command]
async fn get_object(kubri: State<'_, Arc<Kubri>>, target: ObjectRef) -> CommandResult<Value> {
    kubri.get(&target).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn replace_object(
    kubri: State<'_, Arc<Kubri>>,
    target: ObjectRef,
    value: Value,
) -> CommandResult<Value> {
    kubri.replace(&target, value).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn patch_object(kubri: State<'_, Arc<Kubri>>, target: ObjectRef, patch: Value) -> CommandResult<Value> {
    kubri.patch(&target, patch).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_object(
    kubri: State<'_, Arc<Kubri>>,
    target: ObjectRef,
    options: Option<DeleteOptions>,
) -> CommandResult<()> {
    kubri
        .delete(&target, &options.unwrap_or_default())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn scale(kubri: State<'_, Arc<Kubri>>, target: ObjectRef, replicas: u32) -> CommandResult<()> {
    kubri.scale(&target, replicas).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn restart(kubri: State<'_, Arc<Kubri>>, target: ObjectRef) -> CommandResult<()> {
    kubri.restart(&target).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn events(
    kubri: State<'_, Arc<Kubri>>,
    namespace: Option<String>,
    uid: String,
) -> CommandResult<Vec<EventInfo>> {
    kubri
        .events(namespace.as_deref(), &uid)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn metrics(
    kubri: State<'_, Arc<Kubri>>,
    kind: MetricsKind,
    namespace: Option<String>,
) -> CommandResult<Vec<Usage>> {
    kubri
        .metrics(kind, namespace.as_deref())
        .await
        .map_err(|e| e.to_string())
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kubri_desktop_lib=info,kubri_core=info".into()),
        )
        .init();

    tauri::Builder::default()
        .manage(Arc::new(Kubri::load()))
        .manage(Arc::new(Watches::default()))
        .invoke_handler(tauri::generate_handler![
            contexts,
            reload_contexts,
            switch_context,
            resources,
            watch_table,
            unwatch,
            get_object,
            replace_object,
            patch_object,
            delete_object,
            scale,
            restart,
            events,
            metrics
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Kubri");
}
