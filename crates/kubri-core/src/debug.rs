//! Debugging what has no shell or keeps crashing, and shells on nodes, as `kubectl debug` does:
//! an ephemeral container in a pod, a copy of a pod that sleeps instead of crashing, and a
//! privileged pod on a node.

use std::time::Duration;

use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use kube::api::{Patch, PatchParams, PostParams};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Error, Kubri, ObjectRef, ResourceRef, Result, TerminalMode};

/// The image debug containers start with until another one is used.
pub const DEFAULT_IMAGE: &str = "busybox";
/// Marks what Kubri started for debugging: `ephemeral`, `copy` or `node`.
const LABEL: &str = "kubri.dev/debug";
/// The pod a copy was made from.
const COPY_OF: &str = "kubri.dev/debug-copy-of";
/// How long a debug container may take to start, pulling its image included.
const START_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DebugRequest {
    /// An ephemeral container in the pod that sees the processes of `container`. Works for
    /// images without a shell.
    #[serde(rename_all = "camelCase")]
    Ephemeral {
        target: ObjectRef,
        container: Option<String>,
        image: Option<String>,
    },
    /// A copy of the pod in which `container` sleeps instead of running its command, for crash
    /// loops. The copy has no labels, so no Service sends it traffic and no controller adopts it.
    #[serde(rename_all = "camelCase")]
    Copy {
        target: ObjectRef,
        container: Option<String>,
    },
    /// A privileged pod on the node with a shell in the node's namespaces. It is removed when
    /// the shell ends.
    #[serde(rename_all = "camelCase")]
    Node {
        node: String,
        namespace: Option<String>,
        image: Option<String>,
    },
}

/// Where to connect once the debug container runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugSession {
    pub pod: ObjectRef,
    pub container: String,
    pub mode: TerminalMode,
    /// What is left behind, such as a copy to delete when done.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DebugSettings {
    /// The image last used for a debug container.
    pub image: String,
}

impl Default for DebugSettings {
    fn default() -> Self {
        Self {
            image: DEFAULT_IMAGE.into(),
        }
    }
}

fn pods() -> ResourceRef {
    ResourceRef {
        group: String::new(),
        version: "v1".into(),
        plural: "pods".into(),
    }
}

/// An ephemeral container like `kubectl debug -it --image <image> --target <container>`.
/// stdinOnce makes the shell end when the terminal closes.
pub(crate) fn ephemeral_container(name: &str, image: &str, target: Option<&str>) -> Value {
    let mut container = json!({
        "name": name,
        "image": image,
        "stdin": true,
        "stdinOnce": true,
        "tty": true,
        "terminationMessagePolicy": "FallbackToLogsOnError",
    });
    if let Some(target) = target {
        container["targetContainerName"] = target.into();
    }
    container
}

/// A name for a new ephemeral container that no container of `pod` has yet.
fn free_name(pod: &Pod) -> String {
    let spec = pod.spec.as_ref();
    let taken: Vec<&str> = spec
        .into_iter()
        .flat_map(|s| {
            s.containers
                .iter()
                .chain(s.init_containers.iter().flatten())
                .map(|c| c.name.as_str())
                .chain(s.ephemeral_containers.iter().flatten().map(|c| c.name.as_str()))
        })
        .collect();
    (1..)
        .map(|i| {
            if i == 1 {
                "debugger".to_owned()
            } else {
                format!("debugger-{i}")
            }
        })
        .find(|n| !taken.contains(&n.as_str()))
        .unwrap_or_default()
}

/// A copy of `pod` in which `container` sleeps: no labels, owners or node, no probes on the
/// sleeping container, and nothing restarts it.
pub(crate) fn copy_of(pod: &Pod, container: &str) -> Result<Value> {
    let name = pod.metadata.name.clone().unwrap_or_default();
    let mut spec = serde_json::to_value(pod.spec.clone().unwrap_or_default())?;
    let containers = spec
        .get_mut("containers")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| Error::Unsupported(format!("pod {name} has no containers")))?;
    let target = containers
        .iter_mut()
        .find(|c| c.get("name").and_then(Value::as_str) == Some(container))
        .ok_or_else(|| Error::Unsupported(format!("pod {name} has no container {container}")))?;
    let fields = target.as_object_mut().expect("a container is an object");
    for probe in ["livenessProbe", "readinessProbe", "startupProbe", "args"] {
        fields.remove(probe);
    }
    fields.insert("command".into(), json!(["sleep", "infinity"]));
    let spec_fields = spec.as_object_mut().expect("a pod spec is an object");
    for field in ["nodeName", "ephemeralContainers"] {
        spec_fields.remove(field);
    }
    spec_fields.insert("restartPolicy".into(), "Never".into());
    // Names end up in labels, which allow 63 characters.
    let prefix: String = name.chars().take(48).collect();
    Ok(json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "generateName": format!("{prefix}-debug-"),
            "labels": { LABEL: "copy" },
            "annotations": { COPY_OF: name },
        },
        "spec": spec,
    }))
}

/// Enters the node's namespaces with nsenter, or where the runtime refuses that, changes root
/// into the node's file system, as `kubectl debug node/… --profile=sysadmin` and `chroot /host`.
const NODE_SHELL: &str = "if nsenter -t 1 -m -u -i -n -p -- true 2>/dev/null; \
then exec nsenter -t 1 -m -u -i -n -p -- sh -l; \
else echo 'Kubri: nsenter is not allowed here, so this shell runs in chroot /host.'; exec chroot /host sh -l; fi";

/// A privileged pod on `node` with a shell in the node's namespaces.
pub(crate) fn node_pod(node: &str, image: &str) -> Value {
    let prefix: String = node.chars().take(40).collect();
    json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "generateName": format!("node-shell-{prefix}-"),
            "labels": { LABEL: "node" },
        },
        "spec": {
            "nodeName": node,
            "hostPID": true,
            "hostNetwork": true,
            "hostIPC": true,
            "restartPolicy": "Never",
            "terminationGracePeriodSeconds": 0,
            "tolerations": [{ "operator": "Exists" }],
            "volumes": [{ "name": "host", "hostPath": { "path": "/" } }],
            "containers": [{
                "name": "shell",
                "image": image,
                "command": ["sh", "-c", NODE_SHELL],
                "stdin": true,
                "stdinOnce": true,
                "tty": true,
                "terminationMessagePolicy": "FallbackToLogsOnError",
                "securityContext": { "privileged": true },
                "volumeMounts": [{ "name": "host", "mountPath": "/host" }],
            }],
        },
    })
}

/// Why `container` cannot start, once it is clear it will not, such as a wrong image.
fn start_failure(pod: &Pod, container: &str) -> Option<String> {
    let status = pod.status.as_ref()?;
    let state = [&status.container_statuses, &status.ephemeral_container_statuses]
        .into_iter()
        .flatten()
        .flatten()
        .find(|s| s.name == container)?
        .state
        .as_ref()?;
    if let Some(waiting) = &state.waiting {
        let reason = waiting.reason.as_deref().unwrap_or_default();
        if matches!(
            reason,
            "ErrImagePull"
                | "ImagePullBackOff"
                | "InvalidImageName"
                | "CreateContainerError"
                | "CreateContainerConfigError"
        ) {
            return Some(match &waiting.message {
                Some(message) => format!("{reason}: {message}"),
                None => reason.to_owned(),
            });
        }
    }
    let terminated = state.terminated.as_ref()?;
    Some(format!(
        "it ended at once{}",
        terminated
            .message
            .as_deref()
            .or(terminated.reason.as_deref())
            .map(|m| format!(": {m}"))
            .unwrap_or_default()
    ))
}

fn running(pod: &Pod, container: &str) -> bool {
    let Some(status) = &pod.status else { return false };
    [&status.container_statuses, &status.ephemeral_container_statuses]
        .into_iter()
        .flatten()
        .flatten()
        .any(|s| s.name == container && s.state.as_ref().is_some_and(|s| s.running.is_some()))
}

/// Waits until `container` runs, or says why it does not.
async fn wait_running(api: &Api<Pod>, name: &str, container: &str) -> Result<()> {
    let deadline = tokio::time::Instant::now() + START_TIMEOUT;
    loop {
        let pod = api.get(name).await?;
        if running(&pod, container) {
            return Ok(());
        }
        if let Some(reason) = start_failure(&pod, container) {
            return Err(Error::Unsupported(format!(
                "the debug container in {name} did not start: {reason}"
            )));
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(Error::Unsupported(format!(
                "the debug container in {name} did not start within {} seconds",
                START_TIMEOUT.as_secs()
            )));
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Removes the node shell's pod once its shell has ended, or after a day at most.
fn remove_when_done(api: Api<Pod>, name: String) {
    tokio::spawn(async move {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(24 * 3600);
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let done = match api.get_opt(&name).await {
                Ok(Some(pod)) => {
                    let phase = pod.status.and_then(|s| s.phase).unwrap_or_default();
                    phase == "Succeeded" || phase == "Failed"
                }
                Ok(None) => return,
                Err(_) => false,
            };
            if done || tokio::time::Instant::now() >= deadline {
                let _ = api.delete(&name, &Default::default()).await;
                return;
            }
        }
    });
}

/// Removes a node shell's pod when its terminal closes, which ends the shell for good: the
/// shell is not something to come back to. Other terminals pass through.
pub(crate) fn end_with_terminal(api: Api<Pod>, pod: &Pod, mut terminal: crate::Terminal) -> crate::Terminal {
    let is_node_shell = pod
        .metadata
        .labels
        .as_ref()
        .and_then(|l| l.get(LABEL))
        .is_some_and(|v| v == "node");
    if !is_node_shell {
        return terminal;
    }
    struct Remove(Option<(Api<Pod>, String)>);
    impl Drop for Remove {
        fn drop(&mut self) {
            if let Some((api, name)) = self.0.take()
                && let Ok(runtime) = tokio::runtime::Handle::try_current()
            {
                runtime.spawn(async move {
                    let _ = api.delete(&name, &Default::default()).await;
                });
            }
        }
    }
    let guard = Remove(Some((api, pod.metadata.name.clone().unwrap_or_default())));
    terminal.output = futures::StreamExt::boxed(futures::StreamExt::map(terminal.output, move |out| {
        let _ = &guard;
        out
    }));
    terminal
}

fn generated_name(created: &Pod) -> String {
    created.metadata.name.clone().unwrap_or_default()
}

impl Kubri {
    /// Starts a debug container and says where to connect to it.
    pub async fn debug(&self, request: DebugRequest) -> Result<DebugSession> {
        self.ensure_writable().await?;
        let session = self.session().await?;
        let context = Some(session.info.context.clone());
        let image = |image: &Option<String>| {
            image
                .as_deref()
                .map(str::trim)
                .filter(|i| !i.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| self.settings().debug.image)
        };
        let remember = |image: &str| {
            let image = image.to_owned();
            // Remembering is a convenience: a broken settings file does not stop debugging.
            let _ = self.update_settings(|s| {
                s.debug.image = image;
                Ok(())
            });
        };

        match request {
            DebugRequest::Ephemeral {
                target,
                container,
                image: wanted,
            } => {
                let session = self.object_session(&target).await?;
                if !session.is_core(&target.resource, "Pod")? {
                    return Err(Error::Unsupported("debug containers go into pods".into()));
                }
                session
                    .ensure_may("patch", &target, "ephemeralcontainers")
                    .await?;
                session.ensure_may("create", &target, "attach").await?;
                let api = session.pod_api(target.namespace.as_deref())?;
                let pod = api.get(&target.name).await?;
                let image = image(&wanted);
                let name = free_name(&pod);
                let aim = container.or_else(|| crate::terminal::default_container(&pod));
                let patch = json!({ "spec": { "ephemeralContainers": [ephemeral_container(&name, &image, aim.as_deref())] } });
                api.patch_subresource(
                    "ephemeralcontainers",
                    &target.name,
                    &PatchParams::default(),
                    &Patch::Strategic(patch),
                )
                .await?;
                remember(&image);
                wait_running(&api, &target.name, &name).await?;
                Ok(DebugSession {
                    pod: target,
                    container: name,
                    mode: TerminalMode::Attach,
                    note: Some(
                        "Ephemeral containers stay in the pod's spec until the pod is replaced.".into(),
                    ),
                })
            }
            DebugRequest::Copy { target, container } => {
                let session = self.object_session(&target).await?;
                if !session.is_core(&target.resource, "Pod")? {
                    return Err(Error::Unsupported("only pods can be copied for debugging".into()));
                }
                let namespace = target.namespace.clone();
                let new = ObjectRef {
                    context: context.clone(),
                    resource: pods(),
                    namespace: namespace.clone(),
                    name: String::new(),
                };
                session.ensure_may("create", &new, "").await?;
                let api = session.pod_api(namespace.as_deref())?;
                let pod = api.get(&target.name).await?;
                let container = container
                    .or_else(|| crate::terminal::default_container(&pod))
                    .ok_or_else(|| Error::Unsupported(format!("pod {} has no containers", target.name)))?;
                let copy: Pod = serde_json::from_value(copy_of(&pod, &container)?)?;
                let created = api.create(&PostParams::default(), &copy).await?;
                let name = generated_name(&created);
                // An image without `sleep`, such as a distroless one, cannot be copied this way.
                if let Err(err) = wait_running(&api, &name, &container).await {
                    let _ = api.delete(&name, &Default::default()).await;
                    return Err(err);
                }
                Ok(DebugSession {
                    pod: ObjectRef {
                        name: name.clone(),
                        ..new
                    },
                    container,
                    mode: TerminalMode::Shell,
                    note: Some(format!("The copy {name} stays until you delete it.")),
                })
            }
            DebugRequest::Node {
                node,
                namespace,
                image: wanted,
            } => {
                let namespace = namespace
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| "default".into());
                let new = ObjectRef {
                    context: context.clone(),
                    resource: pods(),
                    namespace: Some(namespace.clone()),
                    name: String::new(),
                };
                session.ensure_may("create", &new, "").await?;
                let api = session.pod_api(Some(&namespace))?;
                let image = image(&wanted);
                let pod: Pod = serde_json::from_value(node_pod(&node, &image))?;
                let created = api.create(&PostParams::default(), &pod).await?;
                let name = generated_name(&created);
                remember(&image);
                if let Err(err) = wait_running(&api, &name, "shell").await {
                    let _ = api.delete(&name, &Default::default()).await;
                    return Err(err);
                }
                remove_when_done(api, name.clone());
                Ok(DebugSession {
                    pod: ObjectRef { name, ..new },
                    container: "shell".into(),
                    mode: TerminalMode::Attach,
                    note: Some("The node shell's pod is removed when the shell ends.".into()),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pod(value: Value) -> Pod {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn ephemeral_containers_aim_at_a_container_and_end_with_the_terminal() {
        let c = ephemeral_container("debugger", "busybox", Some("app"));
        assert_eq!(c["targetContainerName"], "app");
        assert_eq!(c["stdinOnce"], true);
        assert_eq!(c["tty"], true);
        assert!(
            ephemeral_container("d", "busybox", None)
                .get("targetContainerName")
                .is_none()
        );
    }

    #[test]
    fn debug_container_names_do_not_clash() {
        let p = pod(json!({ "spec": {
            "containers": [{ "name": "app" }],
            "ephemeralContainers": [{ "name": "debugger" }]
        }}));
        assert_eq!(free_name(&p), "debugger-2");
    }

    #[test]
    fn a_copy_sleeps_and_is_left_alone_by_services_and_controllers() {
        let p = pod(json!({
            "metadata": {
                "name": "web-1", "namespace": "shop", "uid": "u", "resourceVersion": "9",
                "labels": { "app": "web" },
                "ownerReferences": [{ "apiVersion": "apps/v1", "kind": "ReplicaSet", "name": "web", "uid": "r" }]
            },
            "spec": {
                "nodeName": "n1",
                "containers": [
                    { "name": "app", "image": "shop:1", "command": ["/shop"], "args": ["--port=80"],
                      "livenessProbe": { "httpGet": { "port": 80 } } },
                    { "name": "proxy", "image": "envoy", "readinessProbe": { "httpGet": { "port": 9 } } }
                ]
            },
            "status": { "phase": "Running" }
        }));
        let copy = copy_of(&p, "app").unwrap();
        assert_eq!(copy["metadata"]["generateName"], "web-1-debug-");
        assert_eq!(copy["metadata"]["labels"], json!({ LABEL: "copy" }));
        assert!(copy["metadata"].get("ownerReferences").is_none());
        assert!(copy.get("status").is_none());
        let app = &copy["spec"]["containers"][0];
        assert_eq!(app["command"], json!(["sleep", "infinity"]));
        assert!(app.get("args").is_none() && app.get("livenessProbe").is_none());
        // Other containers stay as they were.
        assert!(copy["spec"]["containers"][1].get("readinessProbe").is_some());
        assert!(copy["spec"].get("nodeName").is_none());
        assert_eq!(copy["spec"]["restartPolicy"], "Never");
        assert!(copy_of(&p, "nope").is_err());
    }

    #[test]
    fn node_shells_are_privileged_on_their_node() {
        let p = node_pod("worker-1", "busybox");
        assert_eq!(p["spec"]["nodeName"], "worker-1");
        assert_eq!(p["spec"]["hostPID"], true);
        assert_eq!(p["spec"]["containers"][0]["securityContext"]["privileged"], true);
        assert!(
            p["spec"]["containers"][0]["command"][2]
                .as_str()
                .unwrap()
                .contains("nsenter -t 1")
        );
        assert_eq!(p["metadata"]["labels"][LABEL], "node");
    }

    #[test]
    fn says_why_a_debug_container_does_not_start() {
        let p = pod(json!({ "status": { "ephemeralContainerStatuses": [{
            "name": "debugger", "image": "x", "imageID": "", "ready": false, "restartCount": 0,
            "state": { "waiting": { "reason": "ErrImagePull", "message": "not found" } }
        }]}}));
        assert_eq!(
            start_failure(&p, "debugger").as_deref(),
            Some("ErrImagePull: not found")
        );
        assert!(!running(&p, "debugger"));
    }
}
