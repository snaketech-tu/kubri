//! Tests of `Kubri` against the fake API server in `testing`.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use kube::Client;
use kube::config::Kubeconfig;
use serde_json::Value;

use crate::session::Session;
use crate::{
    ClusterInfo, ConnectionSettings, ContextSettings, DeleteOptions, DrainOptions, Error, HistoryQuery,
    Kubri, LabelTemplate, LocalClusterRequest, LocalClusterTool, LocalTerminalOptions, ObjectRef,
    PortForwardRequest, ResourceInfo, ResourceRef, TerminalSettings, client_config, context_entry, grafana,
    testing,
};

#[tokio::test]
async fn client_config_impersonates_and_proxies_like_kubectl() {
    let kubeconfig = Kubeconfig::from_yaml(
        "clusters:
- {name: c, cluster: {server: 'https://127.0.0.1:6443', proxy-url: 'http://proxy.example:3128'}}
users:
- {name: u, user: {token: t, as: jane, as-uid: '42', as-user-extra: {scopes: [a]}}}
contexts:
- {name: x, context: {cluster: c, user: u}}",
    )
    .unwrap();
    let config = client_config(&kubeconfig, "x", None).await.unwrap();
    // Kubri sends these headers itself, so kube-rs must not send them a second time.
    let count = |name: &str| config.headers.iter().filter(|(k, _)| k == name).count();
    assert_eq!(count("impersonate-uid"), 1);
    assert_eq!(count("impersonate-extra-scopes"), 1);
    assert!(config.auth_info.impersonate_uid.is_none());
    assert!(config.auth_info.impersonate_user_extra.is_none());
    assert_eq!(config.auth_info.impersonate.as_deref(), Some("jane"));
    // A proxy-url in the kubeconfig counts even for localhost.
    assert_eq!(
        config.proxy_url.map(|u| u.to_string()).as_deref(),
        Some("http://proxy.example:3128/")
    );
}

#[tokio::test]
async fn missing_kubeconfig_is_reported_not_fatal() {
    let kubri = Kubri::with_kubeconfig(Err("not found".into()));
    assert!(matches!(kubri.contexts().await, Err(Error::NoKubeconfig(_))));
    assert!(matches!(kubri.resources().await, Err(Error::NotConnected)));
    assert!(kubri.cluster().await.is_none());
}

#[tokio::test]
async fn history_says_why_nothing_is_recorded() {
    let kubri = Kubri::with_kubeconfig(Err("not found".into()));
    assert!(matches!(
        kubri.history(&HistoryQuery::default()).await,
        Err(Error::NotConnected)
    ));
    let query = HistoryQuery {
        context: Some("lab".into()),
        ..Default::default()
    };
    let page = kubri.history(&query).await.unwrap();
    assert!(page.entries.is_empty() && page.watched.is_empty());
    assert_eq!(
        page.recorder.reason.as_deref(),
        Some("This Kubri does not record changes")
    );
    kubri.record_changes();
    let page = kubri.history(&query).await.unwrap();
    assert_eq!(
        page.recorder.reason.as_deref(),
        Some("Kubri records only the context it is connected to")
    );
    assert!(matches!(kubri.history_detail(1), Err(Error::NotFound(_))));
}

#[tokio::test]
async fn started_read_only_is_fixed_and_says_so() {
    let kubri = Kubri::with_kubeconfig(Err("not found".into()));
    kubri
        .set_context_settings(
            "dev",
            ContextSettings {
                read_only: true,
                ..Default::default()
            },
        )
        .unwrap();
    // The setting is a switch in Kubri, so it is not fixed.
    let guard = kubri.guard_for(Some("dev")).await;
    assert!(guard.read_only && !guard.fixed, "{guard:?}");

    kubri.set_read_only();
    let guard = kubri.guard_for(Some("dev")).await;
    assert!(guard.read_only && guard.fixed && !guard.can_unlock, "{guard:?}");
    assert!(guard.reason.unwrap().contains("--read-only"));
}

#[tokio::test]
async fn agents_change_only_contexts_that_allow_it() {
    let kubri = Kubri::with_kubeconfig(Err("not found".into()));
    let allow = |production: bool, locked: bool| ContextSettings {
        agent_writes: true,
        production,
        locked,
        ..Default::default()
    };
    kubri.set_context_settings("dev", allow(false, false)).unwrap();
    kubri.set_context_settings("prod", allow(true, false)).unwrap();
    kubri.set_context_settings("locked", allow(false, true)).unwrap();
    // A person may change all of them, the locked one after unlocking.
    assert!(!kubri.guard_for(Some("dev")).await.read_only);
    assert!(!kubri.guard_for(Some("prod")).await.read_only);

    kubri.set_agent();
    assert!(!kubri.guard_for(Some("dev")).await.read_only);
    for context in ["prod", "locked", "other"] {
        let guard = kubri.guard_for(Some(context)).await;
        assert!(guard.read_only, "{context}");
        assert!(!guard.can_unlock, "{context}");
    }
    assert!(
        kubri
            .guard_for(Some("prod"))
            .await
            .reason
            .unwrap()
            .contains("production")
    );
}

#[tokio::test]
async fn agents_leave_namespaces_alone_by_their_labels() {
    let kubri = Kubri::from_kubeconfig(Kubeconfig::default());
    let (client, requests) = testing::client(|req| {
        let labels = match req.uri.rsplit('/').next().unwrap_or_default() {
            "shop" => r#"{"env":"production"}"#,
            "web" => r#"{"env":"dev"}"#,
            "hidden" => return (403, r#"{"kind":"Status","status":"Failure","code":403}"#.into()),
            _ => return (200, r#"{"kind":"Status","status":"Success"}"#.into()),
        };
        let body = format!(
            r#"{{"kind":"Namespace","apiVersion":"v1","metadata":{{"name":"x","labels":{labels}}}}}"#
        );
        (200, body)
    });
    let info = ClusterInfo {
        context: "dev".into(),
        server: "https://dev".into(),
        version: "v1.36.0".into(),
        default_namespace: "default".into(),
        snapshot: None,
        resumed: false,
    };
    let resources = vec![
        resource("", "v1", "configmaps", "ConfigMap", true),
        resource("", "v1", "namespaces", "Namespace", false),
    ];
    *kubri.session.write().await = Some(Arc::new(Session::new(info, client, resources)));
    kubri
        .set_context_settings(
            "dev",
            ContextSettings {
                agent_writes: true,
                production_namespaces: vec!["*-prod".into()],
                production_labels: vec!["env=production".into()],
                ..Default::default()
            },
        )
        .unwrap();
    let config = |namespace: &str| ObjectRef {
        resource: ResourceRef {
            group: String::new(),
            version: "v1".into(),
            plural: "configmaps".into(),
        },
        namespace: Some(namespace.into()),
        name: "settings".into(),
        context: None,
    };
    let patch = serde_json::json!({ "data": { "a": "b" } });
    kubri.set_agent();
    // By label, by a name pattern, and where the labels cannot be read.
    for refused in ["shop", "checkout-prod", "hidden"] {
        let err = kubri
            .patch(&config(refused), patch.clone())
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("production namespace"), "{refused}: {err}");
    }
    // Nor the Namespace object itself.
    let namespace = ObjectRef {
        resource: ResourceRef {
            group: String::new(),
            version: "v1".into(),
            plural: "namespaces".into(),
        },
        namespace: None,
        name: "checkout-prod".into(),
        context: None,
    };
    let err = kubri
        .delete(&namespace, &DeleteOptions::default())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("production namespace"), "{err}");
    kubri.patch(&config("web"), patch).await.unwrap();
    let sent = requests.lock().unwrap();
    let patched: Vec<_> = sent
        .iter()
        .filter(|r| r.method == "PATCH")
        .map(|r| r.uri.clone())
        .collect();
    assert_eq!(patched.len(), 1, "{patched:?}");
    assert!(patched[0].contains("/namespaces/web/"), "{patched:?}");
    // A name pattern needs no labels.
    assert!(
        !sent.iter().any(|r| r.uri.ends_with("/namespaces/checkout-prod")),
        "{sent:?}"
    );
}

#[tokio::test]
async fn agents_leave_production_namespaces_alone() {
    let kubri = Kubri::from_kubeconfig(Kubeconfig::default());
    let (client, requests) = testing::client(|_| (200, r#"{"kind":"Status","status":"Success"}"#.into()));
    let info = ClusterInfo {
        context: "dev".into(),
        server: "https://dev".into(),
        version: "v1.36.0".into(),
        default_namespace: "default".into(),
        snapshot: None,
        resumed: false,
    };
    let resources = vec![resource("", "v1", "configmaps", "ConfigMap", true)];
    *kubri.session.write().await = Some(Arc::new(Session::new(info, client, resources)));
    kubri
        .set_context_settings(
            "dev",
            ContextSettings {
                agent_writes: true,
                production_namespaces: vec![" shop ".into(), "shop".into(), String::new()],
                ..Default::default()
            },
        )
        .unwrap();
    let settings = kubri.settings().context("dev");
    assert_eq!(settings.production_namespaces, ["shop"]);
    assert!(settings.production_by_name("shop").is_some() && settings.production_by_name("web").is_none());
    let config = |namespace: &str| ObjectRef {
        resource: ResourceRef {
            group: String::new(),
            version: "v1".into(),
            plural: "configmaps".into(),
        },
        namespace: Some(namespace.into()),
        name: "settings".into(),
        context: None,
    };
    let patch = serde_json::json!({ "data": { "a": "b" } });
    // A person still may.
    kubri.patch(&config("shop"), patch.clone()).await.unwrap();
    kubri.set_agent();
    let err = kubri
        .patch(&config("shop"), patch.clone())
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("production namespace"), "{err}");
    kubri.patch(&config("web"), patch).await.unwrap();
    let sent = requests.lock().unwrap();
    assert_eq!(
        sent.iter()
            .filter(|r| r.uri.contains("/namespaces/shop/"))
            .count(),
        1,
        "{sent:?}"
    );
}

fn resource(group: &str, version: &str, plural: &str, kind: &str, namespaced: bool) -> ResourceInfo {
    ResourceInfo {
        reference: ResourceRef {
            group: group.into(),
            version: version.into(),
            plural: plural.into(),
        },
        kind: kind.into(),
        singular: kind.to_lowercase(),
        short_names: Vec::new(),
        namespaced,
        verbs: vec!["list".into(), "watch".into()],
        subresources: Vec::new(),
    }
}

/// A Kubri connected to a fake API server.
async fn connected(client: Client) -> Kubri {
    let kubri = Kubri::from_kubeconfig(Kubeconfig::default());
    let resources = vec![
        resource("", "v1", "pods", "Pod", true),
        resource("", "v1", "configmaps", "ConfigMap", true),
        resource("apps", "v1", "deployments", "Deployment", true),
    ];
    let info = ClusterInfo {
        context: "test".into(),
        server: "https://fake".into(),
        version: "v1.36.0".into(),
        default_namespace: "default".into(),
        snapshot: None,
        resumed: false,
    };
    *kubri.session.write().await = Some(Arc::new(Session::new(info, client, resources)));
    kubri
}

/// A session for `context` on a fake API server that answers `/version` with `status`.
fn fake_session(context: &str, status: u16) -> (Arc<Session>, Arc<std::sync::Mutex<Vec<testing::Recorded>>>) {
    let (client, requests) = testing::client(move |r| {
        assert_eq!(r.uri, "/version", "a kept connection is only checked");
        let body = serde_json::json!({
            "major": "1", "minor": "36", "gitVersion": "v1.36.0", "gitCommit": "", "gitTreeState": "",
            "buildDate": "", "goVersion": "", "compiler": "", "platform": ""
        });
        match status {
            200 => (200, body.to_string()),
            code => (code, testing::status(code, "Unauthorized")),
        }
    });
    let info = ClusterInfo {
        context: context.into(),
        server: format!("https://{context}"),
        version: "v1.36.0".into(),
        default_namespace: "default".into(),
        snapshot: None,
        resumed: false,
    };
    let session = Session::new(info, client, vec![resource("", "v1", "pods", "Pod", true)]);
    (Arc::new(session), requests)
}

async fn current_context(kubri: &Kubri) -> String {
    kubri.cluster().await.unwrap().context
}

#[tokio::test]
async fn switching_back_resumes_a_kept_connection() {
    let kubri = Kubri::from_kubeconfig(Kubeconfig::default());
    let (a, _) = fake_session("a", 200);
    let (b, b_requests) = fake_session("b", 200);
    *kubri.session.write().await = Some(a);
    kubri.keep_warm(b).await;

    // `b` is not in the kubeconfig, so only the kept connection can make this work.
    let info = kubri.switch_context("b").await.unwrap();
    assert!(info.resumed);
    assert_eq!(current_context(&kubri).await, "b");
    assert_eq!(b_requests.lock().unwrap().len(), 1, "one version check");
    assert_eq!(kubri.warm_contexts().await, ["a"]);

    let info = kubri.switch_context("a").await.unwrap();
    assert!(info.resumed);
    assert_eq!(kubri.warm_contexts().await, ["b"]);
    // The current context is not kept twice, and a reconnect is never resumed.
    assert!(matches!(
        kubri.switch_context("a").await,
        Err(Error::UnknownContext(_))
    ));
    assert_eq!(current_context(&kubri).await, "a");
}

#[tokio::test]
async fn a_cancelled_switch_keeps_the_current_connection() {
    let kubri = Kubri::from_kubeconfig(Kubeconfig::default());
    let (a, _) = fake_session("a", 200);
    let (b, _) = fake_session("b", 200);
    *kubri.session.write().await = Some(a);
    kubri.keep_warm(b).await;

    // The cancel lands while the switch waits for the lock, as if it came mid-connect.
    let slot = kubri.session.write().await;
    let switching = kubri.switch_context("b");
    let cancelling = async {
        tokio::task::yield_now().await;
        drop(slot);
        kubri.cancel_switch().await;
    };
    let (switched, ()) = tokio::join!(switching, cancelling);
    assert!(matches!(switched, Err(Error::Cancelled(name)) if name == "b"));
    assert_eq!(current_context(&kubri).await, "a");
    // The connection it made is kept for later.
    assert_eq!(kubri.warm_contexts().await, ["b"]);
    // A cancel with nothing under way changes nothing.
    kubri.cancel_switch().await;
    assert!(kubri.switch_context("b").await.is_ok());
    assert_eq!(current_context(&kubri).await, "b");
}

#[tokio::test]
async fn a_kept_connection_that_fails_is_dropped() {
    let kubri = Kubri::from_kubeconfig(Kubeconfig::default());
    let (a, _) = fake_session("a", 200);
    let (b, _) = fake_session("b", 401);
    *kubri.session.write().await = Some(a);
    kubri.keep_warm(b).await;

    // Kubri connects afresh, which fails here; the current connection stays.
    assert!(matches!(
        kubri.switch_context("b").await,
        Err(Error::UnknownContext(_))
    ));
    assert_eq!(current_context(&kubri).await, "a");
    assert!(kubri.warm_contexts().await.is_empty());
}

#[tokio::test]
async fn keeps_only_as_many_contexts_as_the_setting_says() {
    let kubri = Kubri::from_kubeconfig(Kubeconfig::default());
    for name in ["a", "b", "c", "d", "e"] {
        kubri.keep_warm(fake_session(name, 200).0).await;
    }
    assert_eq!(kubri.warm_contexts().await, ["e", "d", "c"]);
    // Keeping one again moves it to the front instead of adding it twice.
    kubri.keep_warm(fake_session("c", 200).0).await;
    assert_eq!(kubri.warm_contexts().await, ["c", "e", "d"]);

    kubri
        .set_connection_settings(ConnectionSettings { keep_recent: 1 })
        .await
        .unwrap();
    assert_eq!(kubri.warm_contexts().await, ["c"]);
    assert!(
        kubri
            .set_connection_settings(ConnectionSettings { keep_recent: 6 })
            .await
            .is_err()
    );
    kubri
        .set_connection_settings(ConnectionSettings { keep_recent: 0 })
        .await
        .unwrap();
    kubri.keep_warm(fake_session("f", 200).0).await;
    assert!(kubri.warm_contexts().await.is_empty());
}

#[test]
fn terminal_fonts_are_names_only() {
    let kubri = Kubri::from_kubeconfig(Kubeconfig::default());
    let font = |family: &str| TerminalSettings {
        font_family: family.into(),
        ..Default::default()
    };
    let saved = kubri
        .set_terminal_settings(font("  MesloLGS NF, 'Fira Code' "))
        .unwrap();
    assert_eq!(saved.terminal.font_family, "MesloLGS NF, 'Fira Code'");
    assert!(kubri.set_terminal_settings(font("x} body { color: red")).is_err());
    assert!(kubri.set_terminal_settings(font(&"a".repeat(201))).is_err());
}

#[tokio::test]
async fn grafana_shows_the_logs_of_the_object_picked() {
    let kubri = Kubri::from_kubeconfig(Kubeconfig::default());
    let (client, _) = testing::client(|_| {
        (
            200,
            r#"{"kind":"List","apiVersion":"v1","metadata":{},"items":[]}"#.into(),
        )
    });
    let info = ClusterInfo {
        context: "grafana-logs-test".into(),
        server: "https://grafana-logs-test".into(),
        version: "v1.36.0".into(),
        default_namespace: "default".into(),
        snapshot: None,
        resumed: false,
    };
    let resources = vec![
        resource("", "v1", "pods", "Pod", true),
        resource("", "v1", "configmaps", "ConfigMap", true),
        resource("", "v1", "namespaces", "Namespace", false),
        resource("", "v1", "nodes", "Node", false),
        resource("apps", "v1", "deployments", "Deployment", true),
    ];
    let session = Session::new(info, client, resources);
    *kubri.session.write().await = Some(Arc::new(session));
    let object = |group: &str, plural: &str, namespace: Option<&str>, name: &str| ObjectRef {
        resource: ResourceRef {
            group: group.into(),
            version: "v1".into(),
            plural: plural.into(),
        },
        namespace: namespace.map(Into::into),
        name: name.into(),
        context: None,
    };
    let pod = object("", "pods", Some("shop"), "web-1");
    // No Grafana in the cluster: Kubri says where to give one.
    let err = kubri.grafana_logs(&pod).await.unwrap_err().to_string();
    assert!(err.contains("integrations page"), "{err}");

    let mut settings = ContextSettings::default();
    settings.integrations.grafana = "https://grafana.example.com/".into();
    settings.integrations.grafana_logs = " loki-uid ".into();
    kubri.set_context_settings("grafana-logs-test", settings).unwrap();
    let saved = kubri.settings().context("grafana-logs-test").integrations;
    assert_eq!(
        (saved.grafana.as_str(), saved.grafana_logs.as_str()),
        ("https://grafana.example.com", "loki-uid")
    );

    let query = |link: &grafana::DashboardLink| {
        let panes = link.path.strip_prefix("/explore?schemaVersion=1&panes=").unwrap();
        let decoded = percent_decode(panes);
        let pane: serde_json::Value = serde_json::from_str(&decoded).unwrap();
        assert_eq!(pane["kubri"]["datasource"], "loki-uid");
        pane["kubri"]["queries"][0]["expr"].as_str().unwrap().to_owned()
    };
    let logs = kubri.grafana_logs(&pod).await.unwrap();
    assert_eq!(logs.title, "Logs of pod web-1");
    assert_eq!(query(&logs), r#"{namespace="shop", pod="web-1"}"#);
    let logs = kubri
        .grafana_logs(&object("", "namespaces", None, "shop"))
        .await
        .unwrap();
    assert_eq!(logs.title, "Logs of namespace shop");
    assert_eq!(query(&logs), r#"{namespace="shop"}"#);
    let logs = kubri
        .grafana_logs(&object("", "configmaps", Some("shop"), "settings"))
        .await
        .unwrap();
    assert_eq!(logs.title, "Logs of namespace shop");
    let logs = kubri
        .grafana_logs(&object("apps", "deployments", Some("shop"), "web"))
        .await
        .unwrap();
    assert_eq!(logs.title, "Logs of deployment web");
    assert!(query(&logs).starts_with(r#"{namespace="shop", pod=~"web-"#));
    assert!(
        kubri
            .grafana_logs(&object("", "nodes", None, "n1"))
            .await
            .is_err()
    );

    // A template renames the labels, for collectors that use other names.
    let template = LabelTemplate {
        name: " otel ".into(),
        namespace: "exported_namespace".into(),
        pod: "k8s_pod_name".into(),
        container: String::new(),
    };
    let wrong = LabelTemplate {
        namespace: "exported-namespace".into(),
        ..template.clone()
    };
    let err = kubri.set_label_templates(vec![wrong]).unwrap_err().to_string();
    assert!(err.contains("not a label name"), "{err}");
    assert!(
        kubri
            .set_label_templates(vec![template.clone(), template.clone()])
            .is_err()
    );
    kubri.set_label_templates(vec![template]).unwrap();
    let mut settings = kubri.settings().context("grafana-logs-test");
    settings.integrations.labels = "otel".into();
    kubri.set_context_settings("grafana-logs-test", settings).unwrap();
    let logs = kubri.grafana_logs(&pod).await.unwrap();
    assert_eq!(
        query(&logs),
        r#"{exported_namespace="shop", k8s_pod_name="web-1"}"#
    );
    let logs = kubri
        .grafana_logs(&object("", "namespaces", None, "shop"))
        .await
        .unwrap();
    assert_eq!(query(&logs), r#"{exported_namespace="shop"}"#);
    // Once the template is gone, the usual names come back.
    kubri.set_label_templates(Vec::new()).unwrap();
    let logs = kubri.grafana_logs(&pod).await.unwrap();
    assert_eq!(query(&logs), r#"{namespace="shop", pod="web-1"}"#);
}

fn percent_decode(text: &str) -> String {
    let mut out = Vec::new();
    let mut bytes = text.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let hex: String = bytes.by_ref().take(2).map(char::from).collect();
            out.push(u8::from_str_radix(&hex, 16).unwrap());
        } else {
            out.push(byte);
        }
    }
    String::from_utf8(out).unwrap()
}

#[tokio::test]
async fn contexts_that_run_commands_are_added_only_where_local_terminals_are() {
    let kubri = Kubri::from_kubeconfig(Kubeconfig::default());
    let text = r#"
clusters: [{name: c, cluster: {server: "https://10.0.0.1:6443"}}]
users: [{name: u, user: {exec: {apiVersion: client.authentication.k8s.io/v1beta1, command: aws}}}]
contexts: [{name: eks, context: {cluster: c, user: u}}]
"#;
    let found = kubri.inspect_kubeconfig(text).await.unwrap();
    assert!(found[0].problem.as_deref().unwrap().contains("running `aws`"));
    let err = kubri.add_contexts(text, &["eks".into()]).await.unwrap_err();
    assert!(err.to_string().contains("KUBRI_LOCAL_TERMINAL=true"), "{err}");
    kubri.local_terminal.store(true, Ordering::Relaxed);
    let found = kubri.inspect_kubeconfig(text).await.unwrap();
    assert_eq!(found[0].problem, None);
}

#[tokio::test]
async fn local_terminals_open_only_where_changes_are_allowed() {
    let kubri = Kubri::from_kubeconfig(Kubeconfig::default());
    assert!(
        kubri
            .local_terminal_info()
            .await
            .reason
            .unwrap()
            .contains("KUBRI_LOCAL_TERMINAL")
    );
    kubri.local_terminal.store(true, Ordering::Relaxed);
    assert!(
        kubri
            .local_terminal_info()
            .await
            .reason
            .unwrap()
            .contains("not connected")
    );

    let (session, _) = fake_session("prod", 200);
    *kubri.session.write().await = Some(session);
    kubri
        .set_context_settings(
            "prod",
            ContextSettings {
                locked: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        kubri
            .local_terminal_info()
            .await
            .reason
            .unwrap()
            .contains("would get around that")
    );
    assert!(matches!(
        kubri.open_local_terminal(&LocalTerminalOptions::default()).await,
        Err(Error::Unsupported(_))
    ));

    kubri.set_agent();
    assert!(
        kubri
            .local_terminal_info()
            .await
            .reason
            .unwrap()
            .contains("AI agents")
    );
}

#[test]
fn a_context_entry_covers_its_cluster_and_user() {
    let read = |token: &str, server: &str| -> Kubeconfig {
        Kubeconfig::from_yaml(&format!(
            "contexts: [{{name: a, context: {{cluster: c, user: u}}}}, {{name: b, context: {{cluster: c, user: v}}}}]\n\
             clusters: [{{name: c, cluster: {{server: '{server}'}}}}]\n\
             users: [{{name: u, user: {{token: '{token}'}}}}, {{name: v, user: {{token: x}}}}]"
        ))
        .unwrap()
    };
    let before = read("one", "https://a");
    assert!(context_entry(&before, "a").is_some());
    assert!(context_entry(&before, "gone").is_none());
    assert_eq!(
        context_entry(&before, "b"),
        context_entry(&read("two", "https://a"), "b")
    );
    assert_ne!(
        context_entry(&before, "a"),
        context_entry(&read("two", "https://a"), "a")
    );
    assert_ne!(
        context_entry(&before, "b"),
        context_entry(&read("one", "https://b"), "b")
    );
}

fn list(kind: &str, items: Value) -> String {
    serde_json::json!({ "apiVersion": "v1", "kind": format!("{kind}List"), "metadata": {}, "items": items })
        .to_string()
}

#[tokio::test]
async fn drain_checks_evictions_before_cordoning() {
    let (client, requests) = testing::client(|req| {
        let path = req.uri.split('?').next().unwrap_or_default();
        match path {
            "/api/v1/pods" => (
                200,
                list(
                    "Pod",
                    serde_json::json!([{
                        "metadata": { "name": "web-1-a", "namespace": "shop",
                                      "ownerReferences": [{ "apiVersion": "apps/v1", "kind": "ReplicaSet",
                                                            "name": "web-1", "uid": "rs", "controller": true }] },
                        "spec": { "nodeName": "worker", "containers": [{ "name": "app", "image": "web:1" }] },
                        "status": { "phase": "Running" }
                    }]),
                ),
            ),
            "/apis/authorization.k8s.io/v1/selfsubjectaccessreviews" => (
                201,
                serde_json::json!({
                    "apiVersion": "authorization.k8s.io/v1", "kind": "SelfSubjectAccessReview",
                    "metadata": {}, "spec": {}, "status": { "allowed": false }
                })
                .to_string(),
            ),
            _ => (404, testing::status(404, "not found")),
        }
    });
    let kubri = connected(client).await;
    let err = kubri.drain("worker", DrainOptions::default()).await.unwrap_err();
    assert!(
        err.to_string().contains("`create` on `pods/eviction` in `shop`"),
        "{err}"
    );
    assert!(
        !requests.lock().unwrap().iter().any(|r| r.method == "PATCH"),
        "the node must not be cordoned"
    );
}

#[tokio::test]
async fn rejects_objects_bound_to_a_different_context() {
    let (client, requests) = testing::client(|_| panic!("stale target must not reach the cluster"));
    let kubri = connected(client).await;
    let target = ObjectRef {
        context: Some("previous-cluster".into()),
        resource: ResourceRef {
            group: String::new(),
            version: "v1".into(),
            plural: "pods".into(),
        },
        namespace: Some("shop".into()),
        name: "worker".into(),
    };
    assert!(matches!(kubri.get(&target).await, Err(Error::ContextChanged(_))));
    assert!(matches!(
        kubri.delete(&target, &DeleteOptions::default()).await,
        Err(Error::ContextChanged(_))
    ));
    let forward = PortForwardRequest {
        target: target.clone(),
        remote_port: 8080,
        local_port: None,
        address: None,
    };
    assert!(matches!(
        kubri.start_port_forward(forward).await,
        Err(Error::ContextChanged(_))
    ));
    assert!(kubri.port_forwards().is_empty());
    assert!(requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn xray_builds_trees_from_listed_objects() {
    let (client, requests) = testing::client(|req| {
        let path = req.uri.split('?').next().unwrap_or_default();
        let body = match path {
            "/apis/apps/v1/namespaces/shop/deployments" => list(
                "Deployment",
                serde_json::json!([{
                    "metadata": { "name": "web", "namespace": "shop", "uid": "dep" },
                    "spec": { "replicas": 1, "selector": {}, "template": {} },
                    "status": { "readyReplicas": 1 }
                }]),
            ),
            "/apis/apps/v1/namespaces/shop/replicasets" => list(
                "ReplicaSet",
                serde_json::json!([{
                    "metadata": { "name": "web-1", "namespace": "shop", "uid": "rs",
                                  "ownerReferences": [{ "apiVersion": "apps/v1", "kind": "Deployment",
                                                        "name": "web", "uid": "dep", "controller": true }] },
                    "spec": { "replicas": 1, "selector": {} }, "status": { "replicas": 1, "readyReplicas": 1 }
                }]),
            ),
            "/api/v1/namespaces/shop/pods" => list(
                "Pod",
                serde_json::json!([{
                    "metadata": { "name": "web-1-a", "namespace": "shop", "uid": "pod",
                                  "ownerReferences": [{ "apiVersion": "apps/v1", "kind": "ReplicaSet",
                                                        "name": "web-1", "uid": "rs", "controller": true }] },
                    "spec": { "containers": [{ "name": "app", "image": "web:1",
                              "envFrom": [{ "secretRef": { "name": "web" } }] }] },
                    "status": { "phase": "Running" }
                }]),
            ),
            "/api/v1/namespaces/shop/secrets" => serde_json::json!({
                "apiVersion": "meta.k8s.io/v1", "kind": "PartialObjectMetadataList", "metadata": {},
                "items": [{ "apiVersion": "meta.k8s.io/v1", "kind": "PartialObjectMetadata",
                            "metadata": { "name": "web", "namespace": "shop" } }]
            })
            .to_string(),
            // Not allowed to list these: references stay unchecked instead of failing.
            _ => return (403, testing::status(403, "forbidden")),
        };
        (200, body)
    });
    let kubri = connected(client).await;
    let deployments = ResourceRef {
        group: "apps".into(),
        version: "v1".into(),
        plural: "deployments".into(),
    };
    let trees = kubri.xray(&deployments, Some("shop")).await.unwrap();
    assert_eq!(trees.len(), 1);
    let pod = &trees[0].children[0].children[0];
    assert_eq!(pod.id, "Deployment:shop/web/ReplicaSet:web-1/Pod:web-1-a");
    let names: Vec<(&str, &str)> = pod
        .children
        .iter()
        .map(|c| (c.kind.as_str(), c.detail.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("Container", "Pending · web:1"),
            ("ServiceAccount", "not checked"),
            ("Secret", "")
        ]
    );
    // Everything is listed from the API server's cache.
    let cached = requests
        .lock()
        .unwrap()
        .iter()
        .all(|r| r.uri.contains("resourceVersion=0"));
    assert!(cached);

    let configmaps = ResourceRef {
        group: String::new(),
        version: "v1".into(),
        plural: "configmaps".into(),
    };
    let err = kubri.xray(&configmaps, None).await.unwrap_err().to_string();
    assert!(err.starts_with("XRay cannot show ConfigMap"), "{err}");
}

#[tokio::test]
async fn pulse_reports_kinds_it_cannot_list() {
    let (client, _) = testing::client(|req| {
        let path = req.uri.split('?').next().unwrap_or_default();
        match path {
            "/api/v1/pods" => (
                200,
                list(
                    "Pod",
                    serde_json::json!([
                        { "metadata": { "name": "a" }, "status": { "phase": "Running" },
                          "spec": { "containers": [{ "name": "c", "resources": { "requests": { "cpu": "250m" } } }] } },
                        { "metadata": { "name": "b" }, "status": { "phase": "Failed" }, "spec": { "containers": [] } }
                    ]),
                ),
            ),
            "/api/v1/nodes" => (
                200,
                list(
                    "Node",
                    serde_json::json!([{ "metadata": { "name": "n" },
                    "status": { "allocatable": { "cpu": "2", "memory": "4Gi" },
                                "conditions": [{ "type": "Ready", "status": "True" }] } }]),
                ),
            ),
            "/apis/autoscaling/v2/horizontalpodautoscalers" => {
                (403, testing::status(403, "no autoscalers for you"))
            }
            _ => (200, list("Thing", serde_json::json!([]))),
        }
    });
    let pulse = connected(client).await.pulse(None).await.unwrap();
    let kind = |name: &str| pulse.kinds.iter().find(|k| k.kind == name).unwrap();
    assert_eq!((kind("Pod").total, kind("Pod").ok, kind("Pod").failed), (2, 1, 1));
    assert_eq!(kind("Node").ok, 1);
    assert_eq!(
        kind("HorizontalPodAutoscaler").error.as_deref(),
        Some("no autoscalers for you")
    );
    // No metrics-server: requests and capacity are still known.
    assert_eq!(pulse.cpu.used, None);
    assert_eq!(pulse.cpu.requested, Some(250.0));
    assert_eq!(pulse.cpu.capacity, Some(2000.0));
}

#[tokio::test]
async fn switching_to_unknown_context_fails() {
    let kubri = Kubri::from_kubeconfig(Kubeconfig::default());
    assert!(matches!(
        kubri.switch_context("nope").await,
        Err(Error::UnknownContext(name)) if name == "nope"
    ));
}

#[tokio::test]
async fn opening_a_snapshot_keeps_the_live_connection_warm() {
    let (client, _) = testing::client(|_| (404, testing::status(404, "not found")));
    let kubri = connected(client).await;
    let file = serde_json::json!({
        "format": "kubri-snapshot", "version": 1,
        "info": { "context": "prod", "server": "https://prod", "kubernetes": "v1.33.5",
                  "namespace": "shop", "taken": 1_793_000_000_000_i64, "kubri": "1.6.2",
                  "objects": 0, "logs": 0, "leftOut": [] },
        "resources": [], "lists": {}, "logs": {}
    });
    kubri
        .open_snapshot(file.to_string().as_bytes(), None)
        .await
        .unwrap();
    assert_eq!(kubri.warm_contexts().await, ["test"]);
}

#[tokio::test]
async fn a_snapshot_is_read_only_and_says_what_it_is() {
    let kubri = Kubri::from_kubeconfig(Kubeconfig::default());
    let file = serde_json::json!({
        "format": "kubri-snapshot", "version": 1,
        "info": { "context": "prod", "server": "https://prod", "kubernetes": "v1.33.5",
                  "namespace": "shop", "taken": 1_793_000_000_000_i64, "kubri": "0.9.0",
                  "objects": 0, "logs": 0, "leftOut": [] },
        "resources": [], "lists": {}, "logs": {}
    });
    let info = kubri
        .open_snapshot(file.to_string().as_bytes(), Some("prod.kubri".into()))
        .await
        .unwrap();
    assert_eq!(info.context, "prod (snapshot)");
    assert_eq!(info.default_namespace, "shop");
    assert_eq!(
        info.snapshot.as_ref().and_then(|s| s.file.as_deref()),
        Some("prod.kubri")
    );
    let guard = kubri.guard().await;
    assert!(guard.read_only && !guard.can_unlock && guard.fixed, "{guard:?}");
    assert!(matches!(
        kubri.take_snapshot(None).await,
        Err(Error::Unsupported(_))
    ));
    assert!(kubri.open_snapshot(b"not a snapshot", None).await.is_err());
    // A file that does not open leaves the snapshot open.
    assert_eq!(
        kubri.cluster().await.map(|c| c.context).as_deref(),
        Some("prod (snapshot)")
    );
}

/// The demo the start page opens, `ui/static/demo.kubri`, made by `scripts/demo/make.sh`.
#[tokio::test]
async fn the_demo_that_comes_with_kubri_opens() {
    let kubri = Kubri::with_kubeconfig(Err("there is no kubeconfig at ~/.kube/config".into()));
    let demo = include_bytes!("../../../ui/static/demo.kubri");
    let info = kubri
        .open_snapshot(demo, Some("demo.kubri".into()))
        .await
        .unwrap();
    let saved = info.snapshot.unwrap();
    assert_eq!(saved.namespace.as_deref(), Some("shop"));
    assert!(saved.objects > 100, "{} objects", saved.objects);
    assert!(saved.logs > 5, "{} containers with logs", saved.logs);
    assert!(kubri.contexts().await.unwrap().contexts.is_empty());

    // Leaving it with no context to go to: back to no kubeconfig.
    kubri.close_snapshot().await;
    assert!(matches!(kubri.contexts().await, Err(Error::NoKubeconfig(_))));
}

#[tokio::test]
async fn local_clusters_are_made_only_where_local_terminals_are_allowed() {
    let kubri = Kubri::with_kubeconfig(Err("there is no kubeconfig at ~/.kube/config".into()));
    let clusters = kubri.local_clusters().await;
    assert!(!clusters.allowed);
    assert!(clusters.reason.unwrap().contains("KUBRI_LOCAL_TERMINAL"));
    assert_eq!(clusters.tools.len(), 2);
    let request = LocalClusterRequest {
        tool: LocalClusterTool::Kind,
        cols: 80,
        rows: 24,
    };
    assert!(matches!(
        kubri.create_local_cluster(&request).await,
        Err(Error::Unsupported(_))
    ));

    kubri.allow_local_terminal();
    assert!(kubri.local_clusters().await.allowed);
    kubri.set_read_only();
    assert_eq!(
        kubri.local_clusters().await.reason.as_deref(),
        Some("Kubri was started read-only.")
    );
}

#[tokio::test]
async fn a_snapshot_opens_without_a_kubeconfig() {
    let kubri = Kubri::with_kubeconfig(Err("there is no kubeconfig at ~/.kube/config".into()));
    assert!(matches!(kubri.contexts().await, Err(Error::NoKubeconfig(_))));
    let file = serde_json::json!({
        "format": "kubri-snapshot", "version": 1,
        "info": { "context": "prod", "server": "https://prod", "kubernetes": "v1.33.5",
                  "namespace": "shop", "taken": 1_793_000_000_000_i64, "kubri": "1.6.2",
                  "objects": 0, "logs": 0, "leftOut": [] },
        "resources": [], "lists": {}, "logs": {}
    });
    kubri
        .open_snapshot(file.to_string().as_bytes(), None)
        .await
        .unwrap();
    let list = kubri.contexts().await.unwrap();
    assert!(list.contexts.is_empty());
    assert_eq!(
        list.cluster.map(|c| c.context).as_deref(),
        Some("prod (snapshot)")
    );
}
