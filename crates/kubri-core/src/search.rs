//! Finding objects by name, labels and fields across kinds, namespaces and contexts.

use std::sync::Arc;

use futures::StreamExt;
use kube::api::ListParams;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::jsonpath::JsonPath;
use crate::{Error, Kubri, ObjectRef, ResourceInfo, ResourceRef, Result, Session};

/// The kinds searched when none are named: what people look for by name, in the order hits
/// of the same rank are shown, workloads before their pods.
const COMMON: [(&str, &str); 15] = [
    ("apps", "deployments"),
    ("apps", "statefulsets"),
    ("apps", "daemonsets"),
    ("batch", "jobs"),
    ("batch", "cronjobs"),
    ("", "services"),
    ("networking.k8s.io", "ingresses"),
    ("", "pods"),
    ("", "configmaps"),
    ("", "secrets"),
    ("", "persistentvolumeclaims"),
    ("", "serviceaccounts"),
    ("", "nodes"),
    ("", "namespaces"),
    ("gateway.networking.k8s.io", "httproutes"),
];
/// Kinds listed at once, per context.
const AT_ONCE: usize = 8;
const DEFAULT_LIMIT: usize = 300;

/// A comparison of a field with a value, like `status.phase!=Running`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct FieldTest {
    /// A JSONPath like those of custom columns, with or without the leading dot.
    pub path: String,
    /// `=`, `!=`, `~` (regular expression), `!~`, `>`, `<`, `>=` or `<=`.
    pub op: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchRequest {
    /// Part of the name, any case; `/…/` for a regular expression; empty for any name.
    pub text: String,
    /// The kinds to search; empty for the common ones.
    pub kinds: Vec<ResourceRef>,
    /// Every kind that can be listed, CRDs too.
    pub all_kinds: bool,
    /// One namespace; `None` for all, or those the context's settings name.
    pub namespace: Option<String>,
    /// A label selector, like `app=web,tier!=db`.
    pub labels: String,
    pub fields: Vec<FieldTest>,
    /// The contexts to search; empty for the connected one.
    pub contexts: Vec<String>,
    /// The most objects returned; 300 when not set.
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub object: ObjectRef,
    pub kind: String,
    pub created: Option<String>,
}

/// A kind or context that could not be searched, and why.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchProblem {
    pub context: String,
    pub kind: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
    /// More matched than the limit.
    pub truncated: bool,
    /// How many kinds were listed, over all contexts.
    pub searched: usize,
    pub problems: Vec<SearchProblem>,
}

/// How a name is matched.
enum NameTest {
    Any,
    Part(String),
    Pattern(Regex),
}

impl NameTest {
    fn new(text: &str) -> Result<Self> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(Self::Any);
        }
        if let Some(inner) = text.strip_prefix('/').and_then(|t| t.strip_suffix('/'))
            && !inner.is_empty()
        {
            return Regex::new(&format!("(?i){inner}"))
                .map(Self::Pattern)
                .map_err(|e| Error::Unsupported(format!("`{inner}` is not a regular expression: {e}")));
        }
        Ok(Self::Part(text.to_lowercase()))
    }

    fn matches(&self, name: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Part(part) => name.to_lowercase().contains(part),
            Self::Pattern(re) => re.is_match(name),
        }
    }

    /// Lower ranks first: the exact name, then names that start with it, then the rest.
    fn rank(&self, name: &str) -> u8 {
        match self {
            Self::Part(part) if name.to_lowercase() == *part => 0,
            Self::Part(part) if name.to_lowercase().starts_with(part.as_str()) => 1,
            _ => 2,
        }
    }
}

/// A field test, parsed once.
struct Field {
    path: JsonPath,
    op: String,
    value: String,
    pattern: Option<Regex>,
}

impl Field {
    fn new(test: &FieldTest) -> Result<Self> {
        let path = test.path.trim();
        let path = if path.starts_with('.') || path.starts_with('$') || path.starts_with('{') {
            path.to_owned()
        } else {
            format!(".{path}")
        };
        let path = JsonPath::parse(&path).map_err(Error::Unsupported)?;
        let pattern = match test.op.as_str() {
            "~" | "!~" => Some(Regex::new(&format!("(?i){}", test.value)).map_err(|e| {
                Error::Unsupported(format!("`{}` is not a regular expression: {e}", test.value))
            })?),
            "=" | "==" | "!=" | ">" | "<" | ">=" | "<=" => None,
            op => {
                return Err(Error::Unsupported(format!(
                    "`{op}` is not a comparison Kubri knows"
                )));
            }
        };
        Ok(Self {
            path,
            op: test.op.clone(),
            value: test.value.clone(),
            pattern,
        })
    }

    /// Whether `object` passes. A field that is missing equals nothing, so `!=` passes it.
    fn passes(&self, object: &Value) -> bool {
        let texts: Vec<String> = self
            .path
            .find(object)
            .into_iter()
            .filter(|v| !v.is_null())
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .collect();
        let number = |s: &str| s.trim().parse::<f64>().ok();
        let compare = |pass: fn(f64, f64) -> bool| {
            let Some(want) = number(&self.value) else {
                return false;
            };
            texts
                .iter()
                .filter_map(|t| number(t))
                .any(|have| pass(have, want))
        };
        match self.op.as_str() {
            "=" | "==" => texts.contains(&self.value),
            "!=" => texts.iter().all(|t| *t != self.value),
            "~" => texts
                .iter()
                .any(|t| self.pattern.as_ref().is_some_and(|p| p.is_match(t))),
            "!~" => !texts
                .iter()
                .any(|t| self.pattern.as_ref().is_some_and(|p| p.is_match(t))),
            ">" => compare(|a, b| a > b),
            "<" => compare(|a, b| a < b),
            ">=" => compare(|a, b| a >= b),
            "<=" => compare(|a, b| a <= b),
            _ => false,
        }
    }
}

/// The kind in `session` for a kind named in the request, whatever its version there.
fn find_kind<'a>(session: &'a Session, wanted: &ResourceRef) -> Option<&'a ResourceInfo> {
    let same = |r: &&ResourceInfo| r.reference.group == wanted.group && r.reference.plural == wanted.plural;
    session
        .resources
        .iter()
        .filter(same)
        .find(|r| r.reference.version == wanted.version)
        .or_else(|| session.resources.iter().find(same))
}

fn kinds_for<'a>(session: &'a Session, request: &SearchRequest) -> Vec<&'a ResourceInfo> {
    let listable =
        |r: &&ResourceInfo| r.verbs.iter().any(|v| v == "list") && !r.reference.plural.contains('/');
    if request.all_kinds {
        // One version of each kind: the one discovery lists first is the preferred one.
        let mut seen = std::collections::HashSet::new();
        return session
            .resources
            .iter()
            .filter(listable)
            .filter(|r| r.reference.group != "metrics.k8s.io" && r.reference.group != "events.k8s.io")
            .filter(|r| seen.insert((r.reference.group.clone(), r.reference.plural.clone())))
            .collect();
    }
    let wanted: Vec<ResourceRef> = if request.kinds.is_empty() {
        COMMON
            .iter()
            .map(|(group, plural)| ResourceRef {
                group: (*group).to_owned(),
                version: String::new(),
                plural: (*plural).to_owned(),
            })
            .collect()
    } else {
        request.kinds.clone()
    };
    wanted
        .iter()
        .filter_map(|w| find_kind(session, w))
        .filter(listable)
        .collect()
}

impl Kubri {
    /// Objects whose name, labels and fields match, over kinds and contexts.
    pub async fn search(&self, request: &SearchRequest) -> Result<SearchResult> {
        let name = Arc::new(NameTest::new(&request.text)?);
        let fields = Arc::new(
            request
                .fields
                .iter()
                .map(Field::new)
                .collect::<Result<Vec<_>>>()?,
        );
        let limit = if request.limit == 0 {
            DEFAULT_LIMIT
        } else {
            request.limit
        };
        let current = self.session().await?.info.context.clone();
        let contexts = if request.contexts.is_empty() {
            vec![current.clone()]
        } else {
            request.contexts.clone()
        };

        let mut result = SearchResult::default();
        let mut found: Vec<(u8, usize, SearchHit)> = Vec::new();
        for context in &contexts {
            let session = match self.read_session(Some(context)).await {
                Ok(session) => session,
                Err(e) => {
                    result.problems.push(SearchProblem {
                        context: context.clone(),
                        kind: None,
                        message: e.to_string(),
                    });
                    continue;
                }
            };
            let namespaces: Vec<Option<String>> =
                match (&request.namespace, self.allowed_namespaces(&session).await) {
                    (Some(ns), _) => vec![Some(ns.clone())],
                    (None, Ok(Some(allowed))) => allowed.into_iter().map(Some).collect(),
                    _ => vec![None],
                };
            let kinds = kinds_for(&session, request);
            result.searched += kinds.len();
            let mut jobs = Vec::new();
            for (order, kind) in kinds.into_iter().enumerate() {
                let scopes: Vec<Option<String>> = if kind.namespaced {
                    namespaces.clone()
                } else {
                    vec![None]
                };
                for ns in scopes {
                    jobs.push((order, kind.clone(), ns));
                }
            }
            let listed = futures::stream::iter(jobs.into_iter().map(|(order, kind, ns)| {
                let session = session.clone();
                let (name, fields) = (name.clone(), fields.clone());
                let labels = request.labels.clone();
                async move {
                    let outcome = list_kind(&session, &kind, ns.as_deref(), &labels, &name, &fields).await;
                    (order, kind, outcome)
                }
            }))
            .buffer_unordered(AT_ONCE)
            .collect::<Vec<_>>()
            .await;
            for (order, kind, outcome) in listed {
                match outcome {
                    Ok(hits) => found.extend(hits.into_iter().map(|(object, created)| {
                        let rank = name.rank(&object.name);
                        (
                            rank,
                            order,
                            SearchHit {
                                object: ObjectRef {
                                    context: Some(context.clone()),
                                    ..object
                                },
                                kind: kind.kind.clone(),
                                created,
                            },
                        )
                    })),
                    // Kinds one may not list are left out quietly when searching every kind.
                    Err(Error::Kube(kube::Error::Api(e))) if request.all_kinds && e.code == 403 => {}
                    Err(e) => result.problems.push(SearchProblem {
                        context: context.clone(),
                        kind: Some(kind.kind.clone()),
                        message: e.to_string(),
                    }),
                }
            }
        }
        found.sort_by(|a, b| {
            (
                a.0,
                a.1,
                &a.2.object.context,
                &a.2.object.namespace,
                &a.2.object.name,
            )
                .cmp(&(
                    b.0,
                    b.1,
                    &b.2.object.context,
                    &b.2.object.namespace,
                    &b.2.object.name,
                ))
        });
        result.truncated = found.len() > limit;
        result.hits = found.into_iter().take(limit).map(|(_, _, hit)| hit).collect();
        result
            .problems
            .sort_by(|a, b| (&a.context, &a.kind).cmp(&(&b.context, &b.kind)));
        result.problems.dedup();
        Ok(result)
    }
}

type Found = Vec<(ObjectRef, Option<String>)>;

/// The objects of one kind in one namespace, or all, that match. Lists only metadata unless
/// fields are compared.
async fn list_kind(
    session: &Session,
    kind: &ResourceInfo,
    namespace: Option<&str>,
    labels: &str,
    name: &NameTest,
    fields: &[Field],
) -> Result<Found> {
    let api = session.api(&kind.reference, namespace)?;
    let mut params = ListParams::default();
    if !labels.trim().is_empty() {
        params = params.labels(labels.trim());
    }
    let reference = |meta: &kube::api::ObjectMeta| ObjectRef {
        context: None,
        resource: kind.reference.clone(),
        namespace: meta.namespace.clone(),
        name: meta.name.clone().unwrap_or_default(),
    };
    let created = |meta: &kube::api::ObjectMeta| meta.creation_timestamp.as_ref().map(|t| t.0.to_string());
    if fields.is_empty() {
        let list = api.list_metadata(&params).await?;
        return Ok(list
            .items
            .iter()
            .filter(|o| name.matches(o.metadata.name.as_deref().unwrap_or_default()))
            .map(|o| (reference(&o.metadata), created(&o.metadata)))
            .collect());
    }
    let list = api.list(&params).await?;
    Ok(list
        .items
        .into_iter()
        .filter(|o| name.matches(o.metadata.name.as_deref().unwrap_or_default()))
        .filter_map(|o| {
            let value = serde_json::to_value(&o).ok()?;
            fields
                .iter()
                .all(|f| f.passes(&value))
                .then(|| (reference(&o.metadata), created(&o.metadata)))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn field(path: &str, op: &str, value: &str) -> Field {
        Field::new(&FieldTest {
            path: path.into(),
            op: op.into(),
            value: value.into(),
        })
        .unwrap()
    }

    #[test]
    fn matches_names() {
        let part = NameTest::new("Web").unwrap();
        assert!(part.matches("shop-web-1"));
        assert!(!part.matches("api"));
        assert_eq!(part.rank("web"), 0);
        assert_eq!(part.rank("web-1"), 1);
        assert_eq!(part.rank("shop-web"), 2);
        let pattern = NameTest::new("/^web-\\d$/").unwrap();
        assert!(pattern.matches("web-1"));
        assert!(!pattern.matches("web-12"));
        assert!(NameTest::new("/(/").is_err());
        assert!(NameTest::new("  ").unwrap().matches("anything"));
    }

    #[test]
    fn compares_fields() {
        let pod = json!({
            "spec": { "nodeName": "node-1", "containers": [{ "image": "nginx:1.27" }, { "image": "envoy:1.30" }] },
            "status": { "phase": "Pending", "containerStatuses": [{ "restartCount": 7 }] }
        });
        assert!(field("status.phase", "!=", "Running").passes(&pod));
        assert!(!field("status.phase", "=", "Running").passes(&pod));
        assert!(field(".spec.nodeName", "=", "node-1").passes(&pod));
        assert!(field("spec.containers[*].image", "~", "^envoy").passes(&pod));
        assert!(field("spec.containers[*].image", "!~", "redis").passes(&pod));
        assert!(field("status.containerStatuses[*].restartCount", ">", "5").passes(&pod));
        assert!(!field("status.containerStatuses[*].restartCount", "<=", "5").passes(&pod));
        // Missing fields equal nothing.
        assert!(field("spec.priorityClassName", "!=", "high").passes(&pod));
        assert!(!field("spec.priorityClassName", "=", "high").passes(&pod));
        assert!(
            Field::new(&FieldTest {
                path: "a".into(),
                op: "<>".into(),
                value: "b".into()
            })
            .is_err()
        );
    }
}
