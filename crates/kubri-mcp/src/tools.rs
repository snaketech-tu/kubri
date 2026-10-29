//! The tools agents can call, and what each one does with the core.

use std::time::Duration;

use futures::StreamExt;
use kubri_core::{
    AccessCheck, ContainerImage, DeleteOptions, Kubri, LogOptions, ObjectRef, ResourceInfo, TableEvent,
    TableScope,
};
use serde_json::{Value, json};

use crate::hub::Hub;
use kubri_core::redact;

use crate::render;

/// How long a list or log read may take.
const READ_TIMEOUT: Duration = Duration::from_secs(20);
const DEFAULT_ROWS: usize = 100;
const MAX_ROWS: usize = 500;
const DEFAULT_LOG_LINES: i64 = 100;
const MAX_LOG_LINES: i64 = 1000;

pub(crate) struct Tool {
    pub name: &'static str,
    title: &'static str,
    description: &'static str,
    /// Changes the cluster: offered only where a person can confirm it.
    write: bool,
    /// Deletes or replaces something that cannot be brought back as it was.
    destructive: bool,
    params: &'static [Param],
}

struct Param {
    name: &'static str,
    kind: &'static str,
    description: &'static str,
    required: bool,
}

const fn param(name: &'static str, kind: &'static str, description: &'static str, required: bool) -> Param {
    Param {
        name,
        kind,
        description,
        required,
    }
}

const CONTEXT: Param = param(
    "context",
    "string",
    "The kubeconfig context; the user's current one when left out.",
    false,
);
const RESOURCE: Param = param(
    "resource",
    "string",
    "The resource type as kubectl names it: `pods`, `po`, `deploy`, `deployments.apps` or a kind such as `Deployment`.",
    true,
);
const NAME: Param = param("name", "string", "The object's name.", true);
const NAMESPACE: Param = param(
    "namespace",
    "string",
    "The namespace; the context's default namespace when left out. Ignored for cluster-wide resources.",
    false,
);

const TOOLS: &[Tool] = &[
    Tool {
        name: "list_contexts",
        title: "List contexts",
        description: "The contexts in the user's kubeconfig, with the current one, the names and labels the user gave them in Kubri, and whether agents may change them.",
        write: false,
        destructive: false,
        params: &[],
    },
    Tool {
        name: "list_resource_types",
        title: "List resource types",
        description: "The resource types the cluster serves, custom resources included, with their short names and API groups.",
        write: false,
        destructive: false,
        params: &[
            param(
                "filter",
                "string",
                "Only types whose name, kind or group contains this text.",
                false,
            ),
            CONTEXT,
        ],
    },
    Tool {
        name: "list_objects",
        title: "List objects",
        description: "Objects of one resource type with the columns `kubectl get` shows, such as READY, STATUS and RESTARTS for pods.",
        write: false,
        destructive: false,
        params: &[
            RESOURCE,
            param(
                "namespace",
                "string",
                "Only this namespace; all namespaces when left out.",
                false,
            ),
            param(
                "label_selector",
                "string",
                "Such as `app=web` or `tier in (api,web)`.",
                false,
            ),
            param(
                "field_selector",
                "string",
                "Such as `status.phase!=Running` or `spec.nodeName=node-1`.",
                false,
            ),
            param(
                "wide",
                "boolean",
                "Add the columns of `kubectl get -o wide`.",
                false,
            ),
            param(
                "limit",
                "integer",
                "Rows to return, 100 by default and 500 at most.",
                false,
            ),
            param(
                "offset",
                "integer",
                "Rows to skip, to page through long lists.",
                false,
            ),
            CONTEXT,
        ],
    },
    Tool {
        name: "get_object",
        title: "Get an object as YAML",
        description: "One object as YAML, with its status, like `kubectl get -o yaml`. Secret values are hidden unless the user allows them.",
        write: false,
        destructive: false,
        params: &[RESOURCE, NAME, NAMESPACE, CONTEXT],
    },
    Tool {
        name: "get_events",
        title: "Get an object's events",
        description: "The events about one object, newest last, like the end of `kubectl describe`.",
        write: false,
        destructive: false,
        params: &[RESOURCE, NAME, NAMESPACE, CONTEXT],
    },
    Tool {
        name: "get_logs",
        title: "Get logs",
        description: "The last log lines of a pod, or of every pod of a workload (Deployment, StatefulSet, DaemonSet, Job) or Service, each line marked with its pod and container.",
        write: false,
        destructive: false,
        params: &[
            param(
                "resource",
                "string",
                "`pods` (the default) or a workload or Service type such as `deploy`.",
                false,
            ),
            NAME,
            NAMESPACE,
            param(
                "container",
                "string",
                "Only this container; all containers when left out.",
                false,
            ),
            param(
                "previous",
                "boolean",
                "The logs of the previous instance, the one that crashed.",
                false,
            ),
            param(
                "tail_lines",
                "integer",
                "Lines per container, 100 by default and 1000 at most.",
                false,
            ),
            param(
                "since_seconds",
                "integer",
                "Only lines from the last this many seconds.",
                false,
            ),
            CONTEXT,
        ],
    },
    Tool {
        name: "diagnose_pod",
        title: "Diagnose a pod",
        description: "Why a pod fails or does not start: containers that crash or wait and why, exit codes, scheduling problems, warning events and the last lines of the crashed container's log. Start here when a pod is not healthy.",
        write: false,
        destructive: false,
        params: &[
            param("name", "string", "The pod's name.", true),
            NAMESPACE,
            CONTEXT,
        ],
    },
    Tool {
        name: "check_problems",
        title: "Check for problems",
        description: "Kubri's sanitizer, like Popeye: problems and risky settings in pods, workloads, services and more, with a score and a grade.",
        write: false,
        destructive: false,
        params: &[
            param(
                "namespace",
                "string",
                "Only this namespace; the whole cluster when left out.",
                false,
            ),
            CONTEXT,
        ],
    },
    Tool {
        name: "show_tree",
        title: "Show what belongs together",
        description: "XRay: each object of a type down to what it owns and uses, such as a Deployment's ReplicaSets, pods, containers, ConfigMaps and Secrets, with their health.",
        write: false,
        destructive: false,
        params: &[
            param(
                "resource",
                "string",
                "Where the trees start: `deploy` (the default), `sts`, `ds`, `jobs`, `cronjobs`, `svc` or `pods`.",
                false,
            ),
            param(
                "namespace",
                "string",
                "Only this namespace; all namespaces when left out.",
                false,
            ),
            CONTEXT,
        ],
    },
    Tool {
        name: "can_i",
        title: "Check a permission",
        description: "Whether the user may do something, like `kubectl auth can-i`.",
        write: false,
        destructive: false,
        params: &[
            param(
                "verb",
                "string",
                "Such as `get`, `list`, `create`, `delete` or `patch`.",
                true,
            ),
            RESOURCE,
            param(
                "namespace",
                "string",
                "The namespace; all namespaces when left out.",
                false,
            ),
            param("name", "string", "One object's name.", false),
            param(
                "subresource",
                "string",
                "Such as `log`, `exec` or `scale`.",
                false,
            ),
            CONTEXT,
        ],
    },
    Tool {
        name: "cluster_health",
        title: "Cluster health",
        description: "Pulses: how many objects of each kind are healthy, failing or pending, CPU and memory use, and the latest warning events.",
        write: false,
        destructive: false,
        params: &[
            param(
                "namespace",
                "string",
                "Only this namespace; the whole cluster when left out.",
                false,
            ),
            CONTEXT,
        ],
    },
    Tool {
        name: "get_metrics",
        title: "Get metrics",
        description: "CPU, memory, restarts, network and request rates of a pod, workload, node or namespace over the last hours, from the cluster's Prometheus: min, average, max and latest per line, and rollouts in that time.",
        write: false,
        destructive: false,
        params: &[
            RESOURCE,
            NAME,
            NAMESPACE,
            param(
                "hours",
                "integer",
                "How far back, in hours; 1 when left out, at most 720.",
                false,
            ),
            CONTEXT,
        ],
    },
    Tool {
        name: "query_prometheus",
        title: "Query Prometheus",
        description: "Runs a PromQL query over the last hours on the cluster's Prometheus and returns min, average, max and latest of each series.",
        write: false,
        destructive: false,
        params: &[
            param("query", "string", "The PromQL expression.", true),
            param(
                "hours",
                "integer",
                "How far back, in hours; 1 when left out, at most 720.",
                false,
            ),
            CONTEXT,
        ],
    },
    Tool {
        name: "what_changed",
        title: "What changed",
        description: "Changes Kubri recorded in a context over the last hours, newest first: rollouts, image changes, scaling, ConfigMap and Secret edits (keys only), nodes, deletions and warning events. Recorded only while a Kubri window or web server was running; the answer says when it was.",
        write: false,
        destructive: false,
        params: &[
            param(
                "namespace",
                "string",
                "Only this namespace; the whole cluster when left out.",
                false,
            ),
            param(
                "resource",
                "string",
                "Only this resource type, like `deploy` or `configmaps`.",
                false,
            ),
            param("name", "string", "Only objects of this name.", false),
            param(
                "hours",
                "integer",
                "How far back, in hours; 1 when left out, at most 336.",
                false,
            ),
            CONTEXT,
        ],
    },
    Tool {
        name: "scale",
        title: "Scale",
        description: "Sets the replicas of a Deployment, StatefulSet or ReplicaSet. The user confirms it in Kubri first.",
        write: true,
        destructive: false,
        params: &[
            RESOURCE,
            NAME,
            NAMESPACE,
            param("replicas", "integer", "The new number of replicas.", true),
            CONTEXT,
        ],
    },
    Tool {
        name: "restart",
        title: "Restart",
        description: "Restarts a Deployment, StatefulSet or DaemonSet with a rolling update, like `kubectl rollout restart`. The user confirms it in Kubri first.",
        write: true,
        destructive: false,
        params: &[RESOURCE, NAME, NAMESPACE, CONTEXT],
    },
    Tool {
        name: "set_image",
        title: "Set an image",
        description: "Sets the image of one container of a workload or pod, like `kubectl set image`. The user confirms it in Kubri first.",
        write: true,
        destructive: false,
        params: &[
            RESOURCE,
            NAME,
            NAMESPACE,
            param("container", "string", "The container's name.", true),
            param("image", "string", "The new image, such as `nginx:1.27`.", true),
            CONTEXT,
        ],
    },
    Tool {
        name: "delete_object",
        title: "Delete an object",
        description: "Deletes one object. The user confirms it in Kubri first.",
        write: true,
        destructive: true,
        params: &[RESOURCE, NAME, NAMESPACE, CONTEXT],
    },
];

fn definition(tool: &Tool) -> Value {
    let properties: serde_json::Map<String, Value> = tool
        .params
        .iter()
        .map(|p| {
            let kind = if p.kind == "integer" {
                json!({ "type": "integer", "minimum": 0, "description": p.description })
            } else {
                json!({ "type": p.kind, "description": p.description })
            };
            (p.name.to_owned(), kind)
        })
        .collect();
    let required: Vec<&str> = tool
        .params
        .iter()
        .filter(|p| p.required)
        .map(|p| p.name)
        .collect();
    json!({
        "name": tool.name,
        "title": tool.title,
        "description": tool.description,
        "inputSchema": { "type": "object", "properties": properties, "required": required, "additionalProperties": false },
        "annotations": {
            "title": tool.title,
            "readOnlyHint": !tool.write,
            "destructiveHint": tool.destructive,
            "idempotentHint": !tool.write || tool.name != "restart",
            "openWorldHint": false,
        },
    })
}

/// The tool definitions; the change tools only with `writes`.
pub(crate) fn list(writes: bool) -> Vec<Value> {
    TOOLS
        .iter()
        .filter(|t| writes || !t.write)
        .map(definition)
        .collect()
}

pub(crate) fn find(name: &str, writes: bool) -> Option<&'static Tool> {
    TOOLS.iter().find(|t| t.name == name && (writes || !t.write))
}

/// The arguments of one call, with readable errors for the agent.
struct Args<'a>(&'a Value);

impl Args<'_> {
    fn str(&self, name: &str) -> Option<&str> {
        self.0.get(name).and_then(Value::as_str).filter(|s| !s.is_empty())
    }

    fn need(&self, name: &str) -> Result<&str, String> {
        self.str(name).ok_or_else(|| format!("`{name}` is missing"))
    }

    fn bool(&self, name: &str) -> bool {
        self.0.get(name).and_then(Value::as_bool).unwrap_or(false)
    }

    fn int(&self, name: &str) -> Result<Option<i64>, String> {
        match self.0.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v
                .as_i64()
                .filter(|n| *n >= 0)
                .map(Some)
                .ok_or_else(|| format!("`{name}` must be a whole number, 0 or more")),
        }
    }
}

/// The time range of `hours`, one hour by default.
fn hours(args: &Args<'_>) -> Result<kubri_core::Range, String> {
    let hours = args.int("hours")?.unwrap_or(1).clamp(1, 720) as u64;
    Ok(kubri_core::Range {
        seconds: hours * 3600,
        end: None,
    })
}

/// Lines of a chart in a few numbers each, which is what an agent can use.
fn summarize(series: &[kubri_core::Series]) -> Vec<Value> {
    series
        .iter()
        .filter(|s| !s.points.is_empty())
        .map(|s| {
            let values: Vec<f64> = s.points.iter().map(|p| p[1]).collect();
            let round = |v: f64| (v * 1000.0).round() / 1000.0;
            json!({
                "name": s.name,
                "min": round(values.iter().copied().fold(f64::INFINITY, f64::min)),
                "avg": round(values.iter().sum::<f64>() / values.len() as f64),
                "max": round(values.iter().copied().fold(f64::NEG_INFINITY, f64::max)),
                "latest": round(values[values.len() - 1]),
            })
        })
        .collect()
}

fn text(err: impl std::fmt::Display) -> String {
    err.to_string()
}

/// Connects to the context the call names, or stays where it is; the user's current context
/// on the first call.
async fn use_context(kubri: &Kubri, args: &Args<'_>) -> Result<String, String> {
    let current = kubri.cluster().await.map(|c| c.context);
    let wanted = match args.str("context") {
        Some(context) => context.to_owned(),
        None => match current {
            Some(current) => return Ok(current),
            None => {
                let list = kubri.contexts().await.map_err(text)?;
                list.contexts
                    .iter()
                    .find(|c| c.is_default)
                    .or(list.contexts.first())
                    .map(|c| c.name.clone())
                    .ok_or("the kubeconfig has no contexts")?
            }
        },
    };
    if current.as_deref() != Some(&wanted) {
        kubri.switch_context(&wanted).await.map_err(text)?;
    }
    Ok(wanted)
}

/// The resource type a name stands for, as kubectl resolves `po`, `deploy` or `deployments.apps`.
async fn resource(kubri: &Kubri, name: &str) -> Result<ResourceInfo, String> {
    let wanted = name.trim().to_lowercase();
    let resources = kubri.resources().await.map_err(text)?;
    let mut matches: Vec<&ResourceInfo> = resources
        .iter()
        .filter(|r| !r.reference.plural.contains('/'))
        .filter(|r| {
            let group = &r.reference.group;
            let names = [
                r.reference.plural.as_str(),
                r.singular.as_str(),
                &r.kind.to_lowercase(),
            ]
            .into_iter()
            .map(str::to_owned)
            .chain(r.short_names.iter().map(|s| s.to_lowercase()))
            .collect::<Vec<_>>();
            names
                .iter()
                .any(|n| *n == wanted || (!group.is_empty() && format!("{n}.{group}") == wanted))
        })
        .collect();
    // The core group wins, as with kubectl: `events` are the core ones.
    matches.sort_by_key(|r| !r.reference.group.is_empty());
    matches.first().map(|r| (*r).clone()).ok_or_else(|| {
        format!("This cluster serves no resource type `{name}`; list_resource_types shows what it serves.")
    })
}

/// The object a call names, in the namespace it names or the context's default one.
async fn object(
    kubri: &Kubri,
    args: &Args<'_>,
    resource_name: &str,
) -> Result<(ObjectRef, ResourceInfo), String> {
    let info = resource(kubri, resource_name).await?;
    let namespace = if info.namespaced {
        Some(match args.str("namespace") {
            Some(ns) => ns.to_owned(),
            None => kubri
                .cluster()
                .await
                .map(|c| c.default_namespace)
                .unwrap_or_else(|| "default".into()),
        })
    } else {
        None
    };
    let target = ObjectRef {
        context: None,
        resource: info.reference.clone(),
        namespace,
        name: args.need("name")?.to_owned(),
    };
    Ok((target, info))
}

fn place(target: &ObjectRef) -> String {
    match &target.namespace {
        Some(ns) => format!("{ns}/{}", target.name),
        None => target.name.clone(),
    }
}

pub(crate) async fn call(tool: &Tool, kubri: &Kubri, hub: &Hub, arguments: &Value) -> Result<String, String> {
    let args = Args(arguments);
    if tool.name == "list_contexts" {
        return list_contexts(kubri).await;
    }
    let context = use_context(kubri, &args).await?;
    match tool.name {
        "list_resource_types" => list_resource_types(kubri, &args).await,
        "list_objects" => list_objects(kubri, &args).await,
        "get_object" => get_object(kubri, &args).await,
        "get_events" => get_events(kubri, &args).await,
        "get_logs" => get_logs(kubri, &args).await,
        "diagnose_pod" => diagnose_pod(kubri, &args).await,
        "check_problems" => check_problems(kubri, &args).await,
        "show_tree" => show_tree(kubri, &args).await,
        "can_i" => can_i(kubri, &args).await,
        "cluster_health" => {
            let pulse = kubri.pulse(args.str("namespace")).await.map_err(text)?;
            Ok(render::yaml(&pulse))
        }
        "get_metrics" => {
            let (target, _) = object(kubri, &args, args.need("resource")?).await?;
            let charts = kubri.charts(&target, hours(&args)?).await.map_err(text)?;
            let summary: Vec<Value> = charts
                .charts
                .iter()
                .map(|c| json!({ "chart": c.title, "unit": c.unit, "lines": summarize(&c.series), "levels": c.levels }))
                .collect();
            let markers: Vec<Value> = charts
                .markers
                .iter()
                .map(|m| json!({ "at": render::unix_time(m.at), "what": m.label }))
                .collect();
            Ok(render::yaml(&json!({
                "source": charts.source,
                "charts": summary,
                "events": markers,
                "noData": charts.missing,
            })))
        }
        "what_changed" => what_changed(kubri, &args, &context).await,
        "query_prometheus" => {
            let series = kubri
                .prometheus_query(args.need("query")?, hours(&args)?)
                .await
                .map_err(text)?;
            if series.is_empty() {
                return Ok("The query returned no series.".into());
            }
            Ok(render::yaml(&summarize(&series)))
        }
        _ => change(tool, kubri, hub, &args, context).await,
    }
}

async fn list_contexts(kubri: &Kubri) -> Result<String, String> {
    let list = kubri.contexts().await.map_err(text)?;
    let current = kubri.cluster().await.map(|c| c.context);
    let settings = kubri.settings();
    let mut lines = Vec::new();
    for c in &list.contexts {
        let s = settings.context(&c.name);
        let mut notes = Vec::new();
        if current.as_deref() == Some(&c.name) || (current.is_none() && c.is_default) {
            notes.push("current".to_owned());
        }
        if !s.display_name.is_empty() {
            notes.push(format!("called \"{}\" in Kubri", s.display_name));
        }
        if !s.label.is_empty() {
            notes.push(format!("label \"{}\"", s.label));
        }
        if s.production {
            notes.push("production".into());
        }
        notes.push(
            if s.agent_writes && !s.production && !s.read_only {
                "agents may change it, with the user's confirmation"
            } else {
                "read-only for agents"
            }
            .into(),
        );
        let ns = c.namespace.as_deref().unwrap_or("default");
        lines.push(format!("- {} (namespace {ns}; {})", c.name, notes.join(", ")));
    }
    if lines.is_empty() {
        return Err("The kubeconfig has no contexts.".into());
    }
    Ok(lines.join("\n"))
}

async fn list_resource_types(kubri: &Kubri, args: &Args<'_>) -> Result<String, String> {
    let filter = args.str("filter").map(str::to_lowercase);
    let resources = kubri.resources().await.map_err(text)?;
    let mut lines = vec!["KIND\tNAME\tSHORT NAMES\tAPI VERSION\tNAMESPACED".to_owned()];
    for r in resources.iter().filter(|r| !r.reference.plural.contains('/')) {
        let api_version = if r.reference.group.is_empty() {
            r.reference.version.clone()
        } else {
            format!("{}/{}", r.reference.group, r.reference.version)
        };
        let line = format!(
            "{}\t{}\t{}\t{api_version}\t{}",
            r.kind,
            r.reference.plural,
            r.short_names.join(","),
            r.namespaced
        );
        if filter.as_ref().is_none_or(|f| line.to_lowercase().contains(f)) {
            lines.push(line);
        }
    }
    Ok(lines.join("\n"))
}

/// A list as the first answer of a table watch: its columns, its rows and a notice, such as
/// the namespaces it fell back to.
async fn read_table(
    kubri: &Kubri,
    resource: &kubri_core::ResourceRef,
    namespace: Option<&str>,
    scope: TableScope,
) -> Result<(Vec<kubri_core::Column>, Vec<kubri_core::Row>, Option<String>), String> {
    let mut events = kubri
        .watch_table(resource, namespace, scope)
        .await
        .map_err(text)?;
    let mut notice = None;
    let (columns, rows) = tokio::time::timeout(READ_TIMEOUT, async {
        while let Some(event) = events.next().await {
            match event {
                TableEvent::Reset { columns, rows } => return Ok((columns, rows)),
                TableEvent::Error { message } => return Err(message),
                TableEvent::Notice { message } => notice = Some(message),
                _ => {}
            }
        }
        Err("the list ended before it arrived".to_owned())
    })
    .await
    .map_err(|_| "The API server did not answer within 20 seconds.".to_owned())??;
    Ok((columns, rows, notice))
}

async fn list_objects(kubri: &Kubri, args: &Args<'_>) -> Result<String, String> {
    let info = resource(kubri, args.need("resource")?).await?;
    let namespace = args.str("namespace").filter(|_| info.namespaced);
    let scope = TableScope {
        label_selector: args.str("label_selector").map(Into::into),
        field_selector: args.str("field_selector").map(Into::into),
        paths: None,
    };
    let limit = (args.int("limit")?.unwrap_or(DEFAULT_ROWS as i64) as usize).clamp(1, MAX_ROWS);
    let offset = args.int("offset")?.unwrap_or(0) as usize;
    let (columns, mut rows, notice) = read_table(kubri, &info.reference, namespace, scope).await?;
    rows.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));
    let total = rows.len();
    let page: Vec<_> = rows.iter().skip(offset).take(limit).collect();
    let mut out = String::new();
    if let Some(notice) = notice {
        out.push_str(&format!("Note: {notice}.\n\n"));
    }
    if page.is_empty() {
        out.push_str(&match (total, namespace) {
            (0, Some(ns)) => format!("No {} in namespace {ns}.", info.reference.plural),
            (0, None) => format!("No {}.", info.reference.plural),
            _ => format!("No rows after offset {offset}; there are {total}."),
        });
        return Ok(out);
    }
    out.push_str(&render::table(&columns, &page, args.bool("wide")));
    if offset > 0 || offset + page.len() < total {
        out.push_str(&format!(
            "\n\nRows {} to {} of {total}.",
            offset + 1,
            offset + page.len()
        ));
        if offset + page.len() < total {
            out.push_str(&format!(" Pass offset {} for more.", offset + page.len()));
        }
    }
    Ok(out)
}

async fn get_object(kubri: &Kubri, args: &Args<'_>) -> Result<String, String> {
    let (target, _) = object(kubri, args, args.need("resource")?).await?;
    let mut value = kubri.get(&target).await.map_err(text)?;
    redact::noise(&mut value);
    if !kubri.settings().agents.secrets {
        redact::secret(&mut value);
    }
    Ok(render::yaml(&value))
}

async fn what_changed(kubri: &Kubri, args: &Args<'_>, context: &str) -> Result<String, String> {
    let hours = args.int("hours")?.unwrap_or(1).clamp(1, 336);
    let now = render::now_ms();
    let since = now - hours * 3600 * 1000;
    let kind = match args.str("resource") {
        Some(name) => Some(resource(kubri, name).await?.kind),
        None => None,
    };
    let page = kubri
        .history(&kubri_core::HistoryQuery {
            context: Some(context.to_owned()),
            namespace: args.str("namespace").map(Into::into),
            kind,
            name: args.str("name").map(Into::into),
            since: Some(since),
            limit: Some(MAX_ROWS as u32),
            ..Default::default()
        })
        .await
        .map_err(text)?;
    let time = |ms: i64| render::unix_time(ms as f64 / 1000.0);
    let mut lines = Vec::new();
    if page.watched.is_empty() {
        lines.push(format!(
            "Kubri did not watch {context} in the last {hours} h, so it has no record of that time. Changes are recorded only while a Kubri window or web server is connected to the context."
        ));
    } else {
        let spans: Vec<String> = page
            .watched
            .iter()
            .rev()
            .map(|s| format!("{} to {}", time(s.from.max(since)), time(s.to)))
            .collect();
        lines.push(format!(
            "Kubri watched {context} from {}.",
            spans.join(", and from ")
        ));
    }
    if page.entries.is_empty() {
        lines.push("No changes recorded in that time.".into());
    }
    for e in &page.entries {
        let place = match &e.namespace {
            Some(ns) => format!("{ns}/{}", e.name),
            None => e.name.clone(),
        };
        let unwatched = if e.unwatched {
            " (happened while Kubri was not watching, found when it started again)"
        } else {
            ""
        };
        lines.push(format!(
            "{}  {} {}  {}: {}{unwatched}",
            time(e.at),
            e.kind,
            place,
            e.change.as_str(),
            e.summary
        ));
    }
    if page.more {
        lines.push(format!(
            "Only the newest {MAX_ROWS} are shown; narrow the question to see more."
        ));
    }
    Ok(lines.join("\n"))
}

async fn get_events(kubri: &Kubri, args: &Args<'_>) -> Result<String, String> {
    let (target, _) = object(kubri, args, args.need("resource")?).await?;
    let events = kubri.event_lines(&target).await.map_err(text)?;
    if events.is_empty() {
        return Ok(format!(
            "No events about {}; events last an hour by default.",
            place(&target)
        ));
    }
    Ok(events.join("\n"))
}

fn log_options(args: &Args<'_>, container: Option<String>, previous: bool) -> Result<LogOptions, String> {
    Ok(LogOptions {
        container,
        follow: false,
        previous,
        timestamps: false,
        tail_lines: Some(
            args.int("tail_lines")?
                .unwrap_or(DEFAULT_LOG_LINES)
                .clamp(1, MAX_LOG_LINES),
        ),
        since_seconds: args.int("since_seconds")?,
    })
}

async fn get_logs(kubri: &Kubri, args: &Args<'_>) -> Result<String, String> {
    let (target, _) = object(kubri, args, args.str("resource").unwrap_or("pods")).await?;
    let options = log_options(args, args.str("container").map(Into::into), args.bool("previous"))?;
    kubri.log_text(&target, options).await.map_err(text)
}

async fn diagnose_pod(kubri: &Kubri, args: &Args<'_>) -> Result<String, String> {
    let (target, info) = object(kubri, args, "pods").await?;
    let pod = match kubri.get(&target).await {
        Ok(pod) => pod,
        Err(err) if err.api_status() == Some(404) => {
            // Agents often pass the name of the Deployment the pod belongs to.
            let (_, rows, _) = read_table(
                kubri,
                &info.reference,
                target.namespace.as_deref(),
                TableScope::default(),
            )
            .await
            .unwrap_or_default();
            let similar: Vec<&str> = rows
                .iter()
                .map(|r| r.name.as_str())
                .filter(|n| n.starts_with(&target.name.to_lowercase()))
                .take(10)
                .collect();
            return Err(if similar.is_empty() {
                format!("There is no pod {}.", place(&target))
            } else {
                format!(
                    "There is no pod {}. Pods with that prefix: {}.",
                    place(&target),
                    similar.join(", ")
                )
            });
        }
        Err(err) => return Err(text(err)),
    };
    Ok(kubri.pod_report(&target, &pod).await)
}

async fn check_problems(kubri: &Kubri, args: &Args<'_>) -> Result<String, String> {
    let report = kubri.sanitize(args.str("namespace")).await.map_err(text)?;
    let mut out = vec![format!("Score {} of 100, grade {}.", report.score, report.grade)];
    for section in &report.sections {
        let score = section.score.map(|s| s.to_string()).unwrap_or_else(|| "-".into());
        out.push(String::new());
        out.push(format!(
            "{} (score {score}): {} checked, {} ok, {} warnings, {} errors",
            section.kind, section.total, section.ok, section.warning, section.error
        ));
        if let Some(error) = &section.failed {
            out.push(format!("  could not check: {error}"));
        }
        for finding in section.findings.iter().take(30) {
            let name = match &finding.namespace {
                Some(ns) => format!("{ns}/{}", finding.name),
                None => finding.name.clone(),
            };
            for issue in &finding.issues {
                let container = issue
                    .container
                    .as_deref()
                    .map(|c| format!(" [{c}]"))
                    .unwrap_or_default();
                out.push(format!(
                    "  {:?} {name}{container}: {}",
                    issue.level, issue.message
                ));
            }
        }
        if section.findings.len() > 30 {
            out.push(format!("  … and {} more", section.findings.len() - 30));
        }
    }
    Ok(out.join("\n"))
}

async fn show_tree(kubri: &Kubri, args: &Args<'_>) -> Result<String, String> {
    let info = resource(kubri, args.str("resource").unwrap_or("deployments")).await?;
    let nodes = kubri
        .xray(&info.reference, args.str("namespace"))
        .await
        .map_err(text)?;
    fn walk(node: &kubri_core::XrayNode, depth: usize, out: &mut Vec<String>) {
        let detail = if node.detail.is_empty() {
            String::new()
        } else {
            format!(" ({})", node.detail)
        };
        let ns = node
            .namespace
            .as_deref()
            .map(|n| format!("{n}/"))
            .unwrap_or_default();
        out.push(format!(
            "{}{} {ns}{} [{:?}]{detail}",
            "  ".repeat(depth),
            node.kind,
            node.name,
            node.health
        ));
        for child in &node.children {
            walk(child, depth + 1, out);
        }
    }
    let mut out = Vec::new();
    for node in &nodes {
        walk(node, 0, &mut out);
    }
    if out.is_empty() {
        return Ok(format!("No {}.", info.reference.plural));
    }
    Ok(out.join("\n"))
}

async fn can_i(kubri: &Kubri, args: &Args<'_>) -> Result<String, String> {
    let info = resource(kubri, args.need("resource")?).await?;
    let verb = args.need("verb")?;
    let mut check = AccessCheck::new(verb, &info.reference, args.str("namespace"));
    if let Some(sub) = args.str("subresource") {
        check = check.on_subresource(sub);
    }
    if let Some(name) = args.str("name") {
        check = check.named(name);
    }
    let answer = kubri
        .access(std::slice::from_ref(&check), true)
        .await
        .map_err(text)?;
    Ok(match answer.first().copied().flatten() {
        Some(true) => format!("yes: {} is allowed", check.describe()),
        Some(false) => format!("no: {} is not allowed", check.describe()),
        None => "The cluster cannot say.".into(),
    })
}

fn kubectl(context: &str, target: &ObjectRef, args: &[String]) -> String {
    let mut words = vec!["kubectl".to_owned(), "--context".into(), context.to_owned()];
    if let Some(ns) = &target.namespace {
        words.extend(["-n".into(), ns.clone()]);
    }
    words.extend(args.iter().cloned());
    words.join(" ")
}

/// A change: checked against the guard rails, confirmed by the user, then done.
async fn change(
    tool: &Tool,
    kubri: &Kubri,
    hub: &Hub,
    args: &Args<'_>,
    context: String,
) -> Result<String, String> {
    let guard = kubri.guard().await;
    if guard.read_only {
        return Err(format!(
            "Kubri does not let agents change the context {context}: {}.",
            guard.reason.unwrap_or_default()
        ));
    }
    let (target, info) = object(kubri, args, args.need("resource")?).await?;
    let reference = format!("{}/{}", info.reference.plural, target.name);
    let what = format!("{} {}", info.kind, place(&target));
    let (summary, command) = match tool.name {
        "scale" => {
            let replicas = args.int("replicas")?.ok_or("`replicas` is missing")?;
            (
                format!("Scale {what} to {replicas} replicas"),
                vec!["scale".into(), reference, format!("--replicas={replicas}")],
            )
        }
        "restart" => (
            format!("Restart {what}"),
            vec!["rollout".into(), "restart".into(), reference],
        ),
        "set_image" => {
            let container = args.need("container")?;
            let image = args.need("image")?;
            (
                format!("Set the image of container {container} in {what} to {image}"),
                vec![
                    "set".into(),
                    "image".into(),
                    reference,
                    format!("{container}={image}"),
                ],
            )
        }
        "delete_object" => (format!("Delete {what}"), vec!["delete".into(), reference]),
        other => return Err(format!("unknown tool {other}")),
    };
    hub.confirm(
        tool.name,
        Some(context.clone()),
        summary.clone(),
        kubectl(&context, &target, &command),
    )
    .await?;
    match tool.name {
        "scale" => {
            let replicas = args.int("replicas")?.unwrap_or_default();
            let replicas = u32::try_from(replicas).map_err(|_| "too many replicas")?;
            kubri.scale(&target, replicas).await.map_err(text)?;
        }
        "restart" => kubri.restart(&target).await.map_err(text)?,
        "set_image" => {
            let image = ContainerImage {
                name: args.need("container")?.to_owned(),
                image: args.need("image")?.to_owned(),
                init: false,
            };
            kubri.set_images(&target, &[image]).await.map_err(text)?;
        }
        _ => kubri
            .delete(&target, &DeleteOptions::default())
            .await
            .map_err(text)?,
    }
    Ok(format!("Done: {summary}, as the user allowed."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_valid_schema() {
        for tool in TOOLS {
            let definition = definition(tool);
            assert!(tool.name.chars().all(|c| c.is_ascii_lowercase() || c == '_'));
            assert_eq!(definition["inputSchema"]["type"], "object");
            for required in definition["inputSchema"]["required"].as_array().unwrap() {
                assert!(
                    definition["inputSchema"]["properties"]
                        .get(required.as_str().unwrap())
                        .is_some()
                );
            }
        }
        assert!(list(false).len() < list(true).len());
        assert!(find("scale", false).is_none());
        assert!(find("scale", true).is_some());
    }
}
