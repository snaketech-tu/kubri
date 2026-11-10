use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use kube::Client;
use kube::config::Kubeconfig;
use serde::Serialize;

use crate::session::Session;
use crate::{
    CONNECT_TIMEOUT, ClusterInfo, ContextList, Error, KubeconfigFile, Kubri, NewContext, OTHER_SESSION_FOR,
    ResourceInfo, Result, WARM_CHECK_TIMEOUT, WARM_FOR, added, chosen, client_config, context_entry,
    discovery, kubeconfig, settings, team,
};

/// Where Kubernetes mounts a pod's service account.
const SERVICE_ACCOUNT: &str = "/var/run/secrets/kubernetes.io/serviceaccount";

/// A context entry from the merged kubeconfig, as shown in the context switcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ContextInfo {
    pub name: String,
    pub cluster: Option<String>,
    /// The API server's address, as the kubeconfig names it.
    pub server: Option<String>,
    pub user: Option<String>,
    pub namespace: Option<String>,
    /// The kubeconfig's `current-context`.
    pub is_default: bool,
    /// Added in Kubri rather than in the user's kubeconfig, so Kubri can remove it.
    pub added: bool,
}

/// Inside a pod without a kubeconfig, writes one for the pod's service account, with the
/// context `in-cluster`, and returns its path to put in `KUBECONFIG`. Kubri, `helm`, `kubectl`
/// and k9s plugins then all use it. `None` outside a pod, or when there is a kubeconfig.
pub fn write_in_cluster_kubeconfig() -> Option<std::io::Result<PathBuf>> {
    let has_kubeconfig = std::env::var_os("KUBECONFIG").is_some()
        || dirs::home_dir().is_some_and(|home| home.join(".kube").join("config").exists());
    if has_kubeconfig {
        return None;
    }
    let host = std::env::var("KUBERNETES_SERVICE_HOST").ok()?;
    let port = std::env::var("KUBERNETES_SERVICE_PORT").ok()?;
    let kubeconfig = in_cluster(&host, &port, Path::new(SERVICE_ACCOUNT))?;
    let path = std::env::temp_dir().join("kubri-in-cluster.kubeconfig");
    // JSON is YAML too. The file holds only paths: the token stays in its own file, which
    // Kubernetes renews.
    Some(write_new(&path, kubeconfig.to_string().as_bytes()).map(|()| path))
}

/// Replaces `path` with a new file, never writing through a link someone put there.
fn write_new(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    match std::fs::remove_file(path) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => return Err(err),
        _ => {}
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(contents)
}

/// A kubeconfig for the service account mounted in `dir`.
fn in_cluster(host: &str, port: &str, dir: &Path) -> Option<serde_json::Value> {
    let token = dir.join("token");
    if !token.exists() {
        return None;
    }
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    let namespace = std::fs::read_to_string(dir.join("namespace"))
        .ok()
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty());
    Some(serde_json::json!({
        "apiVersion": "v1",
        "kind": "Config",
        "clusters": [{
            "name": "in-cluster",
            "cluster": {
                "server": format!("https://{host}:{port}"),
                "certificate-authority": dir.join("ca.crt"),
            },
        }],
        "users": [{ "name": "service-account", "user": { "tokenFile": token } }],
        "contexts": [{
            "name": "in-cluster",
            "context": { "cluster": "in-cluster", "user": "service-account", "namespace": namespace },
        }],
        "current-context": "in-cluster",
    }))
}

pub(crate) fn list_contexts(
    kubeconfig: &Kubeconfig,
    added: &std::collections::HashSet<String>,
) -> Vec<ContextInfo> {
    let default = kubeconfig.current_context.as_deref();
    let mut contexts: Vec<ContextInfo> = kubeconfig
        .contexts
        .iter()
        .map(|named| {
            let ctx = named.context.as_ref();
            let cluster = ctx.map(|c| c.cluster.clone()).filter(|c| !c.is_empty());
            let server = cluster.as_ref().and_then(|name| {
                let found = kubeconfig.clusters.iter().find(|c| &c.name == name)?;
                found.cluster.as_ref()?.server.clone()
            });
            ContextInfo {
                name: named.name.clone(),
                cluster,
                server,
                user: ctx.and_then(|c| c.user.clone()),
                namespace: ctx.and_then(|c| c.namespace.clone()),
                is_default: Some(named.name.as_str()) == default,
                added: added.contains(&named.name),
            }
        })
        .collect();
    contexts.sort_by(|a, b| a.name.cmp(&b.name));
    contexts
}

impl Kubri {
    /// Re-reads the kubeconfig from disk, keeping the current connection.
    pub async fn reload(&self) -> Result<ContextList> {
        let fresh = kubeconfig::read();
        let old = std::mem::replace(&mut *self.kubeconfig.write().await, fresh.clone());
        // Kept connections stay only for contexts whose entries did not change.
        let same = |context: &str| match (&old, &fresh) {
            (Ok(old), Ok(fresh)) => {
                let entry = context_entry(old, context);
                entry.is_some() && entry == context_entry(fresh, context)
            }
            _ => false,
        };
        self.others.lock().await.retain(|context, _| same(context));
        self.warm.lock().await.retain(|(_, s)| same(&s.info.context));
        self.fleet
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|context, _| same(context));
        self.contexts().await
    }

    pub async fn contexts(&self) -> Result<ContextList> {
        let cluster = self.cluster().await;
        let contexts = match &*self.kubeconfig.read().await {
            Ok(kubeconfig) => list_contexts(kubeconfig, &added::names()),
            // A snapshot opened at start needs no kubeconfig: it shows with no context to switch to.
            Err(_) if cluster.as_ref().is_some_and(|c| c.snapshot.is_some()) => Vec::new(),
            Err(err) => return Err(Error::NoKubeconfig(err.clone())),
        };
        Ok(ContextList {
            contexts,
            cluster,
            warm: self.warm_contexts().await,
        })
    }

    /// The contexts of a kubeconfig the user picked, and why any cannot be added.
    pub async fn inspect_kubeconfig(&self, text: &str) -> Result<Vec<NewContext>> {
        self.ensure_not_team(team::NO_ADDED_CONTEXTS)?;
        let new = added::parse(text)?;
        let local = self.local_terminal.load(Ordering::Relaxed);
        Ok(added::inspect(&new, &self.context_names().await, local))
    }

    /// Adds `names` from a kubeconfig the user picked, in Kubri's own folder.
    pub async fn add_contexts(&self, text: &str, names: &[String]) -> Result<ContextList> {
        self.ensure_not_team(team::NO_ADDED_CONTEXTS)?;
        let dir = added::dir()
            .ok_or_else(|| Error::Unsupported("Kubri has no config folder to keep contexts in".into()))?;
        let new = added::parse(text)?;
        let local = self.local_terminal.load(Ordering::Relaxed);
        added::add(&dir, &new, names, &self.context_names().await, local)?;
        self.reload().await
    }

    /// Removes a context added in Kubri. Contexts from the user's kubeconfig stay.
    pub async fn remove_context(&self, name: &str) -> Result<ContextList> {
        self.ensure_not_team(team::NO_ADDED_CONTEXTS)?;
        if self.cluster().await.is_some_and(|c| c.context == name) {
            return Err(Error::Unsupported(format!(
                "Kubri is connected to {name}; switch to another context first"
            )));
        }
        let removed = match added::dir() {
            Some(dir) => added::remove(&dir, name)?,
            None => false,
        };
        if !removed {
            return Err(Error::Unsupported(format!(
                "{name} is in your kubeconfig, which Kubri does not change"
            )));
        }
        // `reload` forgets every kept connection, this one included.
        self.reload().await
    }

    /// Every kubeconfig file Kubri reads: the user's own, then the ones chosen in Kubri.
    pub fn kubeconfig_files(&self) -> Vec<KubeconfigFile> {
        chosen::describe(&kubeconfig::own_paths(), &chosen::files())
    }

    /// Reads the kubeconfig files at `paths` from now on, where they are, after the user's
    /// own. Each must be a kubeconfig Kubri can read, or none is added.
    pub async fn add_kubeconfig_files(&self, paths: &[String]) -> Result<ContextList> {
        self.ensure_not_team(team::NO_ADDED_CONTEXTS)?;
        chosen::add(&kubeconfig::own_paths(), paths)?;
        self.reload().await
    }

    /// Stops reading a file chosen in Kubri. The user's own files stay.
    pub async fn remove_kubeconfig_file(&self, path: &str) -> Result<ContextList> {
        self.ensure_not_team(team::NO_ADDED_CONTEXTS)?;
        let file = self.kubeconfig_files().into_iter().find(|f| f.path == path);
        if let (Some(file), Some(cluster)) = (&file, self.cluster().await)
            && file.chosen
            && file.contexts.contains(&cluster.context)
            && !file.shadowed.contains(&cluster.context)
        {
            return Err(Error::Unsupported(format!(
                "Kubri is connected to {}, which comes from this file; switch to another context first",
                cluster.context
            )));
        }
        if !chosen::remove(path)? {
            return Err(Error::Unsupported(match file {
                Some(_) => format!("{path} is one of your own kubeconfig files, which Kubri does not change"),
                None => format!("Kubri does not read {path}"),
            }));
        }
        self.reload().await
    }

    pub(crate) async fn context_names(&self) -> std::collections::HashSet<String> {
        match &*self.kubeconfig.read().await {
            Ok(kubeconfig) => kubeconfig.contexts.iter().map(|c| c.name.clone()).collect(),
            Err(_) => Default::default(),
        }
    }

    pub async fn cluster(&self) -> Option<ClusterInfo> {
        self.session.read().await.as_ref().map(|s| s.info.clone())
    }

    /// Connects to `context` and discovers the resource types it serves.
    /// The previous connection stays active if this fails.
    ///
    /// A context used a moment ago is taken from the connections kept warm, after a quick
    /// check that it still answers; switching to the current context always connects afresh.
    pub async fn switch_context(&self, context: &str) -> Result<ClusterInfo> {
        let ticket = self.switches.fetch_add(1, Ordering::SeqCst) + 1;
        let current = self.session.read().await.clone();
        let again = current.as_ref().is_some_and(|s| s.info.context == context);
        let kept = if again {
            None
        } else {
            self.take_kept(context).await
        };
        let (session, resumed) = match kept {
            Some(session) => (session, true),
            None => (Arc::new(self.connect(context).await?), false),
        };
        {
            let mut slot = self.session.write().await;
            // Cancelled, or another switch started meanwhile: the current connection stays.
            if self.switches.load(Ordering::SeqCst) != ticket {
                drop(slot);
                // Kept, so that switching there later is instant.
                if !again {
                    self.keep_warm(session).await;
                }
                return Err(Error::Cancelled(context.to_owned()));
            }
            *slot = Some(session.clone());
        }
        if let Some(left) = current.filter(|_| !again) {
            self.keep_warm(left).await;
        }
        let mut info = session.info.clone();
        info.resumed = resumed;
        if resumed {
            tracing::info!(context, version = %info.version, "resumed a kept connection");
        } else {
            tracing::info!(context, version = %info.version, "connected");
        }
        self.restart_recorder(&session).await;
        Ok(info)
    }

    /// Stops waiting for a switch under way: the connection it makes is dropped, and the
    /// current one stays. Without a switch under way, it does nothing.
    pub async fn cancel_switch(&self) {
        // Under the lock, so that a switch either sees this or has finished before it.
        let _slot = self.session.write().await;
        self.switches.fetch_add(1, Ordering::SeqCst);
    }

    /// Runs discovery again for the connected context and returns its resource types. A
    /// switch to a kept connection uses the types found when it was made; this catches what
    /// changed since, such as a newly installed CRD.
    pub async fn refresh_resources(&self) -> Result<Vec<ResourceInfo>> {
        let session = self.session().await?;
        if session.info.snapshot.is_some() {
            return Ok(session.resources.clone());
        }
        let resources = discovery::discover(&session.client).await?;
        if resources == session.resources {
            return Ok(resources);
        }
        let mut fresh = Session::new(session.info.clone(), session.client.clone(), resources.clone());
        fresh.proxy_client = session.proxy_client.clone();
        let mut current = self.session.write().await;
        // Only if no other switch happened meanwhile.
        if current.as_ref().is_some_and(|c| Arc::ptr_eq(c, &session)) {
            *current = Some(Arc::new(fresh));
            tracing::info!(context = session.info.context, "resource types changed");
        }
        Ok(resources)
    }

    /// The contexts kept connected in the background, most recent first.
    pub async fn warm_contexts(&self) -> Vec<String> {
        let mut warm = self.warm.lock().await;
        self.prune_warm(&mut warm);
        warm.iter().map(|(_, s)| s.info.context.clone()).collect()
    }

    /// Keeps the session just left, unless keeping is off or it is a snapshot.
    pub(crate) async fn keep_warm(&self, session: Arc<Session>) {
        if session.info.snapshot.is_some() {
            return;
        }
        let mut warm = self.warm.lock().await;
        warm.retain(|(_, s)| s.info.context != session.info.context);
        warm.insert(0, (std::time::Instant::now(), session));
        self.prune_warm(&mut warm);
    }

    /// Drops kept sessions past the setting's count or unused for [`WARM_FOR`].
    pub(crate) fn prune_warm(&self, warm: &mut Vec<(std::time::Instant, Arc<Session>)>) {
        let keep = self
            .settings()
            .connections
            .keep_recent
            .min(settings::MAX_KEEP_RECENT) as usize;
        warm.retain(|(left, _)| left.elapsed() < WARM_FOR);
        warm.truncate(keep);
    }

    /// A kept session for `context` that still answers, taken out of the pool. A session that
    /// does not answer quickly, or whose credentials stopped working, is dropped so the caller
    /// connects afresh and reports the real error.
    pub(crate) async fn take_kept(&self, context: &str) -> Option<Arc<Session>> {
        let session = {
            let mut warm = self.warm.lock().await;
            self.prune_warm(&mut warm);
            match warm.iter().position(|(_, s)| s.info.context == context) {
                Some(at) => Some(warm.remove(at).1),
                None => {
                    let mut others = self.others.lock().await;
                    others.retain(|_, (made, _)| made.elapsed() < OTHER_SESSION_FOR);
                    others.get(context).map(|(_, s)| s.clone())
                }
            }
        }?;
        match tokio::time::timeout(WARM_CHECK_TIMEOUT, session.client.apiserver_version()).await {
            Ok(Ok(_)) => Some(session),
            Ok(Err(err)) => {
                tracing::info!(context, "a kept connection failed, connecting afresh: {err}");
                None
            }
            Err(_) => {
                tracing::info!(context, "a kept connection did not answer, connecting afresh");
                None
            }
        }
    }

    /// A connection to `context`, without making it the current one.
    pub(crate) async fn connect(&self, context: &str) -> Result<Session> {
        let kubeconfig = match &*self.kubeconfig.read().await {
            Ok(kubeconfig) => kubeconfig.clone(),
            Err(err) => return Err(Error::NoKubeconfig(err.clone())),
        };
        let named = kubeconfig
            .contexts
            .iter()
            .find(|c| c.name == context)
            .ok_or_else(|| Error::UnknownContext(context.to_owned()))?;
        let namespace = named
            .context
            .as_ref()
            .and_then(|c| c.namespace.clone())
            .unwrap_or_else(|| "default".to_owned());

        let session = tokio::time::timeout(CONNECT_TIMEOUT, async {
            let config = client_config(&kubeconfig, context, self.act_as.as_ref()).await?;
            let server = config.cluster_url.to_string().trim_end_matches('/').to_owned();
            let mut once = config.clone();
            once.default_retry = false;
            let proxy_client = Client::try_from(once)?;
            let client = Client::try_from(config)?;
            let version = client.apiserver_version().await?.git_version;
            let resources = discovery::discover(&client).await?;
            let mut session = Session::new(
                ClusterInfo {
                    context: context.to_owned(),
                    server,
                    version,
                    default_namespace: namespace,
                    snapshot: None,
                    resumed: false,
                },
                client,
                resources,
            );
            session.proxy_client = proxy_client;
            Ok::<_, Error>(session)
        })
        .await
        .map_err(|_| Error::Timeout)??;
        Ok(session)
    }

    /// The kubeconfig entry of the connected context.
    pub(crate) async fn context_info(&self) -> Option<ContextInfo> {
        let name = self.cluster().await?.context;
        match &*self.kubeconfig.read().await {
            Ok(kubeconfig) => list_contexts(kubeconfig, &Default::default())
                .into_iter()
                .find(|c| c.name == name),
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KUBECONFIG: &str = r#"
apiVersion: v1
kind: Config
current-context: prod
clusters:
  - name: prod-cluster
    cluster: { server: "https://prod.example.com" }
  - name: dev-cluster
    cluster: { server: "https://dev.example.com" }
users:
  - name: admin
    user: { token: "secret" }
contexts:
  - name: prod
    context: { cluster: prod-cluster, user: admin, namespace: web }
  - name: dev
    context: { cluster: dev-cluster, user: admin }
"#;

    #[test]
    fn uses_the_service_account_in_a_pod() {
        let dir = crate::k9s::tests::scratch("in-cluster");
        assert!(in_cluster("10.96.0.1", "443", &dir).is_none());
        std::fs::write(dir.join("token"), "token").unwrap();
        std::fs::write(dir.join("namespace"), "kubri\n").unwrap();

        let json = in_cluster("10.96.0.1", "443", &dir).unwrap().to_string();
        let kubeconfig = Kubeconfig::from_yaml(&json).unwrap();
        let contexts = list_contexts(&kubeconfig, &Default::default());
        assert_eq!(contexts[0].name, "in-cluster");
        assert_eq!(contexts[0].namespace.as_deref(), Some("kubri"));
        assert!(contexts[0].is_default);
        let cluster = kubeconfig.clusters[0].cluster.as_ref().unwrap();
        assert_eq!(cluster.server.as_deref(), Some("https://10.96.0.1:443"));
        assert!(
            cluster
                .certificate_authority
                .as_deref()
                .unwrap()
                .ends_with("ca.crt")
        );
        let user = kubeconfig.auth_infos[0].auth_info.as_ref().unwrap();
        assert!(user.token_file.as_deref().unwrap().ends_with("token"));

        // Written again on every start, as a new file.
        let path = dir.join("kubeconfig");
        write_new(&path, b"old").unwrap();
        write_new(&path, json.as_bytes()).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), json);

        let ipv6 = in_cluster("fd00::1", "6443", &dir).unwrap();
        assert_eq!(ipv6["clusters"][0]["cluster"]["server"], "https://[fd00::1]:6443");
    }

    #[test]
    fn lists_contexts_sorted_with_default_marked() {
        let kubeconfig = Kubeconfig::from_yaml(KUBECONFIG).unwrap();
        let contexts = list_contexts(&kubeconfig, &Default::default());
        assert_eq!(contexts.len(), 2);
        assert_eq!(contexts[0].name, "dev");
        assert!(!contexts[0].is_default);
        assert_eq!(contexts[0].namespace, None);
        assert_eq!(contexts[1].name, "prod");
        assert!(contexts[1].is_default);
        assert_eq!(contexts[1].cluster.as_deref(), Some("prod-cluster"));
        assert_eq!(contexts[1].server.as_deref(), Some("https://prod.example.com"));
        assert_eq!(contexts[1].namespace.as_deref(), Some("web"));
    }
}
