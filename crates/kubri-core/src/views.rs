//! Declarative views: YAML files that teach Kubri a kind, usually a CRD, without code. A view
//! adds list columns, a health state, detail sections, actions that run commands like k9s
//! plugins, and relations that the XRay graph draws.
//!
//! Kubri ships views for the tools it integrates with; files in `views/` in Kubri's config folder
//! add more, and replace a shipped view of the same kind.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Health;
use crate::jsonpath::{JsonPath, text};
use crate::k9s::{Plugin, PluginInput};

/// The views Kubri ships, by file name.
const BUILT_IN: [(&str, &str); 3] = [
    ("argo-cd.yaml", include_str!("../views/argo-cd.yaml")),
    ("cert-manager.yaml", include_str!("../views/cert-manager.yaml")),
    ("flux.yaml", include_str!("../views/flux.yaml")),
];

/// One kind's view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct View {
    /// The API group; empty for the core group.
    #[serde(default)]
    pub group: String,
    pub kind: String,
    /// Columns added to the list, after the server's.
    #[serde(default)]
    pub columns: Vec<ViewField>,
    /// Rules for the health state; the first that matches decides.
    #[serde(default)]
    pub health: Vec<HealthRule>,
    /// Sections on the Overview tab of the details.
    #[serde(default)]
    pub details: Vec<DetailSection>,
    /// Commands offered in the object's menu, like k9s plugins.
    #[serde(default)]
    pub actions: Vec<ViewAction>,
    /// Objects this kind refers to, drawn as lines in the XRay graph.
    #[serde(default)]
    pub relations: Vec<Relation>,
    /// The file it was read from, or `built-in: <file>`.
    #[serde(default, skip_deserializing)]
    pub source: String,
}

/// A value read from each object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewField {
    pub name: String,
    /// A JSONPath like `.status.conditions[?(@.type=="Ready")].message`.
    pub path: String,
}

/// When an object is in a health state. Without a test, a rule matches when the path finds
/// anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HealthRule {
    pub path: String,
    /// Some value at the path is this.
    #[serde(default)]
    pub equals: Option<String>,
    /// Values were found and none is this.
    #[serde(default)]
    pub not_equals: Option<String>,
    /// Some value at the path matches this regular expression.
    #[serde(default)]
    pub matches: Option<String>,
    /// The path finds something (true) or nothing (false).
    #[serde(default)]
    pub exists: Option<bool>,
    pub state: HealthState,
    /// Shown with the state: a JSONPath when it starts with `.`, otherwise the text itself.
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HealthState {
    Ok,
    Warning,
    Failed,
}

impl From<HealthState> for Health {
    fn from(state: HealthState) -> Self {
        match state {
            HealthState::Ok => Health::Ok,
            HealthState::Warning => Health::Warning,
            HealthState::Failed => Health::Failed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DetailSection {
    pub title: String,
    pub fields: Vec<ViewField>,
}

/// A command for the object's menu, run like a k9s plugin: `$NAME`, `$NAMESPACE`, `$CONTEXT`
/// and the other k9s variables are replaced in `args`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewAction {
    pub name: String,
    #[serde(default)]
    pub shortcut: String,
    #[serde(default)]
    pub description: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Runs without a terminal, and Kubri does not wait for it.
    #[serde(default)]
    pub background: bool,
    /// Shows the command and asks before running it.
    #[serde(default)]
    pub confirm: bool,
    /// Changes the cluster: not offered while Kubri is read-only.
    #[serde(default)]
    pub dangerous: bool,
    #[serde(default)]
    pub inputs: Vec<PluginInput>,
}

/// Another object an object refers to, such as a Certificate's Secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Relation {
    /// The other object's kind, or a JSONPath to it, like `.spec.sourceRef.kind`.
    pub kind: String,
    /// A JSONPath to its name.
    pub name: String,
    /// A JSONPath to its namespace; the object's own namespace without one.
    #[serde(default)]
    pub namespace: Option<String>,
}

/// Every view in use, and what could not be read.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewSet {
    pub views: Vec<View>,
    /// Files or views that could not be read, and why.
    pub errors: Vec<String>,
    /// The folder Kubri reads views from.
    pub dir: Option<String>,
}

/// A view applied to one object, for its details.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderedView {
    pub health: Option<Judged>,
    pub sections: Vec<RenderedSection>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Judged {
    pub health: Health,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderedSection {
    pub title: String,
    /// Each field's name and value; fields whose path finds nothing are left out.
    pub fields: Vec<(String, String)>,
}

/// The folder user views are read from.
pub(crate) fn dir() -> Option<PathBuf> {
    crate::settings::dir().map(|d| d.join("views"))
}

/// The shipped views, then the user's from `dir`, file by file in name order. A view of the
/// same group and kind as an earlier one replaces it.
pub(crate) fn load(dir: Option<&Path>) -> ViewSet {
    let mut set = ViewSet {
        dir: dir.map(|d| d.display().to_string()),
        ..Default::default()
    };
    let mut files: Vec<(String, String)> = BUILT_IN
        .iter()
        .map(|(name, text)| (format!("built-in: {name}"), (*text).to_owned()))
        .collect();
    if let Some(dir) = dir {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("yaml" | "yml")))
            .collect();
        paths.sort();
        for path in paths {
            match std::fs::read_to_string(&path) {
                Ok(text) => files.push((path.display().to_string(), text)),
                Err(err) => set.errors.push(format!("{}: {err}", path.display())),
            }
        }
    }
    let mut at: HashMap<(String, String), usize> = HashMap::new();
    for (source, text) in files {
        let views: Vec<View> = match serde_saphyr::from_multiple(&text) {
            Ok(views) => views,
            Err(err) => {
                set.errors.push(format!("{source}: {err}"));
                continue;
            }
        };
        for mut view in views {
            view.source = source.clone();
            if let Err(err) = view.check() {
                set.errors.push(format!("{source}: {}: {err}", view.kind));
                continue;
            }
            let key = (view.group.clone(), view.kind.clone());
            match at.get(&key) {
                Some(&i) => set.views[i] = view,
                None => {
                    at.insert(key, set.views.len());
                    set.views.push(view);
                }
            }
        }
    }
    set
}

fn path(text: &str) -> Result<JsonPath, String> {
    JsonPath::parse(text)
}

/// A message or kind: a JSONPath when it starts with `.`, otherwise the text itself.
fn is_path(text: &str) -> bool {
    text.starts_with('.') || text.starts_with('{')
}

impl View {
    /// Whether every path and expression parses.
    fn check(&self) -> Result<(), String> {
        if self.kind.is_empty() {
            return Err("it names no kind".into());
        }
        let fields = self
            .columns
            .iter()
            .chain(self.details.iter().flat_map(|s| &s.fields));
        for field in fields {
            path(&field.path)?;
        }
        for rule in &self.health {
            path(&rule.path)?;
            if let Some(pattern) = &rule.matches {
                Regex::new(pattern).map_err(|e| format!("`{pattern}` is not a regular expression: {e}"))?;
            }
            if let Some(message) = rule.message.as_deref().filter(|m| is_path(m)) {
                path(message)?;
            }
        }
        for relation in &self.relations {
            path(&relation.name)?;
            if is_path(&relation.kind) {
                path(&relation.kind)?;
            }
            if let Some(namespace) = &relation.namespace {
                path(namespace)?;
            }
        }
        for action in &self.actions {
            if action.name.is_empty() || action.command.is_empty() {
                return Err("an action needs a name and a command".into());
            }
        }
        Ok(())
    }

    pub(crate) fn is_for(&self, group: &str, kind: &str) -> bool {
        self.group == group && self.kind == kind
    }

    /// The health of an object by the first rule that matches; `None` when none does.
    pub(crate) fn judge(&self, object: &Value) -> Option<Judged> {
        self.health.iter().find_map(|rule| {
            let found: Vec<String> = JsonPath::parse(&rule.path)
                .ok()?
                .find(object)
                .into_iter()
                .filter(|v| !v.is_null())
                .map(text)
                .collect();
            let matched = if let Some(value) = &rule.equals {
                found.iter().any(|f| f == value)
            } else if let Some(value) = &rule.not_equals {
                !found.is_empty() && found.iter().all(|f| f != value)
            } else if let Some(pattern) = &rule.matches {
                let re = Regex::new(pattern).ok()?;
                found.iter().any(|f| re.is_match(f))
            } else {
                rule.exists.unwrap_or(true) == !found.is_empty()
            };
            matched.then(|| Judged {
                health: rule.state.into(),
                message: rule.message.as_deref().and_then(|m| read_text(m, object)),
            })
        })
    }

    /// The text of the Health column: the state, and for problems what the rule says.
    pub(crate) fn health_cell(&self, object: &Value) -> Value {
        match self.judge(object) {
            None => Value::Null,
            Some(Judged {
                health: Health::Ok, ..
            }) => "Healthy".into(),
            Some(Judged { health, message }) => {
                let state = if health == Health::Failed {
                    "Failed"
                } else {
                    "Warning"
                };
                match message {
                    Some(message) => format!("{state}: {message}").into(),
                    None => state.into(),
                }
            }
        }
    }

    pub(crate) fn render(&self, object: &Value) -> RenderedView {
        RenderedView {
            health: self.judge(object),
            sections: self
                .details
                .iter()
                .map(|section| RenderedSection {
                    title: section.title.clone(),
                    fields: section
                        .fields
                        .iter()
                        .filter_map(|f| {
                            let value = JsonPath::parse(&f.path).ok()?.cell(object);
                            value.as_str().map(|v| (f.name.clone(), v.to_owned()))
                        })
                        .collect(),
                })
                .filter(|s| !s.fields.is_empty())
                .collect(),
        }
    }

    /// The objects an object refers to: kind, namespace (`None` for its own) and name.
    pub(crate) fn related(&self, object: &Value) -> Vec<(String, Option<String>, String)> {
        let mut found = Vec::new();
        for relation in &self.relations {
            let Some(kind) = read_text(&relation.kind, object) else {
                continue;
            };
            let namespace = relation.namespace.as_deref().and_then(|n| read_text(n, object));
            let Ok(names) = JsonPath::parse(&relation.name) else {
                continue;
            };
            for name in names.find(object).into_iter().filter_map(Value::as_str) {
                found.push((kind.clone(), namespace.clone(), name.to_owned()));
            }
        }
        found
    }

    /// The actions as k9s plugins, scoped to the kind's resource.
    pub(crate) fn plugins(&self, plural: &str) -> Vec<Plugin> {
        self.actions
            .iter()
            .map(|a| Plugin {
                // Apart from k9s plugins and other kinds' actions of the same name.
                name: format!("{}: {}", self.kind, a.name),
                short_cut: a.shortcut.clone(),
                description: if a.description.is_empty() {
                    a.name.clone()
                } else {
                    a.description.clone()
                },
                scopes: vec![plural.to_owned()],
                command: a.command.clone(),
                args: a.args.clone(),
                background: a.background,
                confirm: a.confirm,
                dangerous: a.dangerous,
                override_: false,
                inputs: a.inputs.clone(),
            })
            .collect()
    }
}

/// Text or the first value a JSONPath finds.
fn read_text(text: &str, object: &Value) -> Option<String> {
    if !is_path(text) {
        return Some(text.to_owned());
    }
    match JsonPath::parse(text).ok()?.cell(object) {
        Value::String(s) if !s.is_empty() => Some(s),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn certificate(ready: &str) -> Value {
        json!({
            "kind": "Certificate",
            "metadata": { "name": "web", "namespace": "shop" },
            "spec": { "secretName": "web-tls", "dnsNames": ["shop.example.com", "www.example.com"],
                      "issuerRef": { "name": "letsencrypt", "kind": "ClusterIssuer" } },
            "status": { "notAfter": "2027-01-01T00:00:00Z", "conditions": [
                { "type": "Ready", "status": ready, "message": "Certificate is up to date" }
            ] }
        })
    }

    #[test]
    fn ships_views_that_read() {
        let set = load(None);
        assert!(set.errors.is_empty(), "{:?}", set.errors);
        let cert = set
            .views
            .iter()
            .find(|v| v.is_for("cert-manager.io", "Certificate"))
            .unwrap();
        assert_eq!(cert.source, "built-in: cert-manager.yaml");
        assert_eq!(cert.health_cell(&certificate("True")), json!("Healthy"));
        assert_eq!(
            cert.health_cell(&certificate("False")),
            json!("Failed: Certificate is up to date")
        );
        assert_eq!(
            cert.related(&certificate("True")),
            [
                ("Secret".to_owned(), None, "web-tls".to_owned()),
                ("ClusterIssuer".to_owned(), None, "letsencrypt".to_owned())
            ]
        );
        for kind in ["Kustomization", "HelmRelease", "GitRepository", "Application"] {
            assert!(set.views.iter().any(|v| v.kind == kind), "{kind}");
        }
    }

    #[test]
    fn user_views_replace_shipped_ones_and_errors_are_kept() {
        let dir = std::env::temp_dir().join(format!("kubri-views-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("a.yaml"),
            r#"
group: cert-manager.io
kind: Certificate
columns:
  - name: Secret
    path: .spec.secretName
details:
  - title: Names
    fields:
      - { name: DNS names, path: ".spec.dnsNames[*]" }
      - { name: Missing, path: .spec.nothing }
actions:
  - name: Renew
    shortcut: Shift-R
    command: cmctl
    args: [renew, $NAME, -n, $NAMESPACE]
    confirm: true
    dangerous: true
---
group: example.com
kind: Widget
health:
  - { path: .status.phase, matches: "^(Broken|Lost)$", state: failed, message: .status.reason }
  - { path: .spec.paused, equals: "true", state: warning, message: Paused }
  - { path: .status.phase, exists: true, state: ok }
"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("b.yml"),
            "kind: Gadget\ncolumns:\n  - { name: X, path: '.a[' }\n",
        )
        .unwrap();
        std::fs::write(dir.join("c.yaml"), "kind: [not, a, view").unwrap();
        std::fs::write(dir.join("notes.txt"), "not read").unwrap();
        let set = load(Some(&dir));
        std::fs::remove_dir_all(&dir).ok();

        assert_eq!(set.errors.len(), 2, "{:?}", set.errors);
        assert!(set.errors[0].contains("b.yml: Gadget: `.a[`"), "{:?}", set.errors);
        assert!(set.errors[1].contains("c.yaml"), "{:?}", set.errors);
        let cert = set
            .views
            .iter()
            .filter(|v| v.kind == "Certificate")
            .collect::<Vec<_>>();
        assert_eq!(cert.len(), 1);
        let cert = cert[0];
        assert!(cert.source.ends_with("a.yaml"));
        assert!(cert.health.is_empty(), "replaced, not merged");
        assert_eq!(
            cert.render(&certificate("True")),
            RenderedView {
                health: None,
                sections: vec![RenderedSection {
                    title: "Names".into(),
                    fields: vec![("DNS names".into(), "shop.example.com,www.example.com".into())]
                }]
            }
        );
        let plugin = &cert.plugins("certificates")[0];
        assert_eq!(plugin.name, "Certificate: Renew");
        assert_eq!(
            (plugin.description.as_str(), plugin.short_cut.as_str()),
            ("Renew", "Shift-R")
        );
        assert_eq!(plugin.scopes, ["certificates"]);
        assert!(plugin.confirm && plugin.dangerous);

        let widget = set.views.iter().find(|v| v.kind == "Widget").unwrap();
        let judge = |object: Value| widget.judge(&object).map(|j| (j.health, j.message));
        assert_eq!(
            judge(json!({ "status": { "phase": "Lost", "reason": "node gone" } })),
            Some((Health::Failed, Some("node gone".into())))
        );
        assert_eq!(
            judge(json!({ "spec": { "paused": true }, "status": { "phase": "Running" } })),
            Some((Health::Warning, Some("Paused".into())))
        );
        assert_eq!(
            judge(json!({ "status": { "phase": "Running" } })),
            Some((Health::Ok, None))
        );
        assert_eq!(judge(json!({ "status": {} })), None);
    }
}
