//! Kubri's own settings: names, colors and guard rails per context, and preferences.
//!
//! They live in `settings.json` in Kubri's config folder, next to the k9s one
//! (`$KUBRI_CONFIG_DIR`, or `kubri` in `$XDG_CONFIG_HOME` or the system's config folder).
//! Nothing is ever written into the kubeconfig.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// What Kubri remembers about one context.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ContextSettings {
    /// Shown instead of the context's name when set.
    pub display_name: String,
    /// A label such as `Staging`, shown beside the name.
    pub label: String,
    /// The accent color, as `#rrggbb`; empty for none.
    pub color: String,
    /// Destructive actions ask to type the object's name, and a red stripe and a badge say where you are.
    pub production: bool,
    /// Namespaces treated like a production context, in a context that is not one: typed names
    /// before destructive actions, a red marker, and no changes by AI agents. Each is a name or a
    /// pattern where `*` stands for any text, like `*-prod`.
    pub production_namespaces: Vec<String>,
    /// Namespaces treated the same by their labels: each entry a selector such as
    /// `env=production` (`key=value`, `key!=value`, `key` or `!key`, joined by commas to need all).
    pub production_labels: Vec<String>,
    /// Kubri never changes this cluster until this is turned off again.
    pub read_only: bool,
    /// Starts read-only; unlocking lasts 15 minutes at a time.
    pub locked: bool,
    /// The namespace to show after connecting; empty for all namespaces.
    pub namespace: String,
    /// The only namespaces to show; empty for all. Lists across namespaces cover these, one
    /// watch each, which also suits users who may not list everywhere.
    pub namespaces: Vec<String>,
    /// Namespaces to show by label, like `team=shop`, when `namespaces` is empty.
    pub namespace_selector: String,
    /// AI agents connected over MCP may change this context, each change confirmed in Kubri.
    /// Production contexts never allow it.
    pub agent_writes: bool,
    /// Port-forwards kept to start with one click, some of them whenever Kubri connects.
    pub forwards: Vec<FavoriteForward>,
    /// Where Prometheus, Grafana and Loki are, when not found on their own.
    pub integrations: crate::IntegrationSettings,
}

/// A port-forward kept to start again on the same local port, with one click or whenever Kubri
/// connects to its context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteForward {
    /// A pod, Service or workload; the context comes from where the favorite is kept.
    pub target: crate::ObjectRef,
    pub remote_port: u16,
    pub local_port: u16,
    /// Where to listen; empty for 127.0.0.1.
    #[serde(default)]
    pub address: String,
    /// Starts whenever Kubri connects to the context. Favorites kept before this choice existed
    /// did, so a missing value means yes.
    #[serde(default = "yes")]
    pub start_on_connect: bool,
}

fn yes() -> bool {
    true
}

impl FavoriteForward {
    /// Whether `forward` is this favorite running.
    pub fn matches(&self, forward: &crate::PortForwardInfo) -> bool {
        forward.target.resource == self.target.resource
            && forward.target.namespace == self.target.namespace
            && forward.target.name == self.target.name
            && forward.remote_port == self.remote_port
            && forward.local_port == self.local_port
    }
}

impl ContextSettings {
    /// The name or pattern in [`Self::production_namespaces`] that marks `namespace`, its own
    /// name first.
    pub fn production_by_name(&self, namespace: &str) -> Option<&str> {
        let names = &self.production_namespaces;
        names
            .iter()
            .find(|p| *p == namespace)
            .or_else(|| {
                names
                    .iter()
                    .find(|p| p.contains('*') && name_matches(p, namespace))
            })
            .map(String::as_str)
    }

    /// The selector in [`Self::production_labels`] that a namespace with `labels` matches. Labels
    /// that could not be read match the first one: better a namespace too careful than one
    /// that lets an agent change production.
    pub fn production_by_labels(&self, labels: Option<&BTreeMap<String, String>>) -> Option<&str> {
        self.production_labels
            .iter()
            .find(|selector| labels.is_none_or(|labels| labels_match(selector, labels)))
            .map(String::as_str)
    }

    fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Trims what was typed and refuses colors the UI could not use.
    fn normalized(mut self) -> Result<Self> {
        self.display_name = self.display_name.trim().to_owned();
        self.label = self.label.trim().to_owned();
        self.namespace = self.namespace.trim().to_owned();
        let mut production = Vec::new();
        for ns in std::mem::take(&mut self.production_namespaces) {
            let ns = ns.trim().to_owned();
            if !ns.is_empty() && !production.contains(&ns) {
                production.push(ns);
            }
        }
        production.sort();
        self.production_namespaces = production;
        let mut labels = Vec::new();
        for selector in std::mem::take(&mut self.production_labels) {
            let Some(requirements) = parse_selector(&selector) else {
                return Err(Error::Unsupported(format!(
                    "the label rule `{}` is not of the form key=value, key!=value, key or !key",
                    selector.trim()
                )));
            };
            let selector = requirements
                .iter()
                .map(Requirement::to_string)
                .collect::<Vec<_>>()
                .join(",");
            if !requirements.is_empty() && !labels.contains(&selector) {
                labels.push(selector);
            }
        }
        self.production_labels = labels;
        self.color = self.color.trim().to_ascii_lowercase();
        self.integrations = self.integrations.normalized()?;
        let hex = self.color.strip_prefix('#').unwrap_or("");
        if !self.color.is_empty() && (hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit())) {
            return Err(Error::Unsupported(format!(
                "the color `{}` is not of the form #rrggbb",
                self.color
            )));
        }
        Ok(self)
    }
}

/// Whether `name` matches `pattern`, where `*` stands for any text, also none.
pub fn name_matches(pattern: &str, name: &str) -> bool {
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or_default();
    let Some(mut rest) = name.strip_prefix(first) else {
        return false;
    };
    let mut parts: Vec<&str> = parts.collect();
    let Some(last) = parts.pop() else {
        return rest.is_empty();
    };
    for part in parts {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    rest.len() >= last.len() && rest.ends_with(last)
}

/// One part of a label selector.
#[derive(Debug, PartialEq, Eq)]
enum Requirement<'a> {
    Equals(&'a str, &'a str),
    Differs(&'a str, &'a str),
    Exists(&'a str),
    Missing(&'a str),
}

impl std::fmt::Display for Requirement<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Equals(key, value) => write!(f, "{key}={value}"),
            Self::Differs(key, value) => write!(f, "{key}!={value}"),
            Self::Exists(key) => write!(f, "{key}"),
            Self::Missing(key) => write!(f, "!{key}"),
        }
    }
}

/// The parts of a selector such as `env=production,!canary`; None when one is not of a form
/// Kubri matches.
fn parse_selector(selector: &str) -> Option<Vec<Requirement<'_>>> {
    let mut out = Vec::new();
    for part in selector.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        out.push(if let Some((k, v)) = part.split_once("!=") {
            Requirement::Differs(label_key(k)?, label_value(v)?)
        } else if let Some((k, v)) = part.split_once("==").or_else(|| part.split_once('=')) {
            Requirement::Equals(label_key(k)?, label_value(v)?)
        } else if let Some(k) = part.strip_prefix('!') {
            Requirement::Missing(label_key(k)?)
        } else {
            Requirement::Exists(label_key(part)?)
        });
    }
    Some(out)
}

fn label_key(key: &str) -> Option<&str> {
    let key = key.trim();
    let ok = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c));
    ok.then_some(key)
}

fn label_value(value: &str) -> Option<&str> {
    let value = value.trim();
    value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        .then_some(value)
}

/// Whether `labels` meet every part of `selector`; never for a selector Kubri cannot read.
pub fn labels_match(selector: &str, labels: &BTreeMap<String, String>) -> bool {
    match parse_selector(selector) {
        Some(requirements) if !requirements.is_empty() => requirements.iter().all(|r| match r {
            Requirement::Equals(k, v) => labels.get(*k).is_some_and(|l| l == v),
            Requirement::Differs(k, v) => labels.get(*k).is_none_or(|l| l != v),
            Requirement::Exists(k) => labels.contains_key(*k),
            Requirement::Missing(k) => !labels.contains_key(*k),
        }),
        _ => false,
    }
}

/// How a list shows one kind: which columns, in which order, and extra JSONPath columns.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ColumnSettings {
    /// Columns not shown, by name.
    pub hidden: Vec<String>,
    /// Columns of the wide view shown without it.
    pub shown: Vec<String>,
    /// Column names in the order to show them; columns not named follow in their own order.
    pub order: Vec<String>,
    /// Extra columns read from each object, like `kubectl get -o custom-columns`.
    pub custom: Vec<CustomColumn>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CustomColumn {
    pub name: String,
    /// A JSONPath such as `.spec.nodeName` or `.spec.containers[*].image`.
    pub path: String,
}

impl ColumnSettings {
    fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Trims names and checks every path, so a list never starts with one it cannot read.
    fn normalized(mut self) -> Result<Self> {
        for column in &mut self.custom {
            column.name = column.name.trim().to_owned();
            column.path = column.path.trim().to_owned();
            if column.name.is_empty() {
                column.name = column.path.clone();
            }
            crate::jsonpath::JsonPath::parse(&column.path).map_err(Error::Unsupported)?;
        }
        let mut seen = std::collections::HashSet::new();
        if let Some(twice) = self.custom.iter().find(|c| !seen.insert(c.path.clone())) {
            return Err(Error::Unsupported(format!(
                "the column {} is there twice",
                twice.path
            )));
        }
        Ok(self)
    }
}

/// Which actions show a server-side dry-run diff before they run. YAML edits always do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DiffSettings {
    pub scale: bool,
    pub restart: bool,
    pub image: bool,
    pub labels: bool,
}

impl Default for DiffSettings {
    fn default() -> Self {
        Self {
            scale: false,
            restart: false,
            image: true,
            labels: true,
        }
    }
}

/// What AI agents connected to Kubri's MCP server may do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentSettings {
    /// Serve MCP over HTTP on localhost, from the running app. Over stdio it is always there.
    pub http: bool,
    /// The port of that endpoint in the desktop app; the web server uses its own port.
    pub port: u16,
    /// Agents may read the values of Secrets; otherwise they see only the keys.
    pub secrets: bool,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            http: false,
            port: 7879,
            secrets: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Per context, by the context's name.
    pub contexts: BTreeMap<String, ContextSettings>,
    pub diff: DiffSettings,
    pub agents: AgentSettings,
    pub explain: crate::ExplainSettings,
    pub debug: crate::DebugSettings,
    /// Per kind, by `group/Kind` (`/Pod` for the core group).
    pub columns: BTreeMap<String, ColumnSettings>,
    /// Searches kept in the sidebar, in their order.
    pub searches: Vec<SavedSearch>,
    /// What the change history records and keeps.
    pub history: crate::HistorySettings,
    /// How Kubri keeps connections to contexts.
    pub connections: ConnectionSettings,
    /// The local terminal's shell, folder and font.
    pub terminal: crate::TerminalSettings,
    /// Label names for logs, kept once and picked per context on the integrations page.
    pub label_templates: Vec<LabelTemplate>,
}

/// The label names a log collector uses for the namespace, pod and container, where they are
/// not `namespace`, `pod` and `container`, such as `exported_namespace`. Empty keeps the usual.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LabelTemplate {
    pub name: String,
    pub namespace: String,
    pub pod: String,
    pub container: String,
}

impl LabelTemplate {
    /// Trimmed, with a name, and only label names Loki and Prometheus take.
    pub(crate) fn normalized(mut self) -> Result<Self> {
        self.name = self.name.trim().to_owned();
        if self.name.is_empty() {
            return Err(Error::Unsupported("a label template needs a name".into()));
        }
        for label in [&mut self.namespace, &mut self.pod, &mut self.container] {
            *label = label.trim().to_owned();
            let mut chars = label.chars();
            let valid = chars.next().is_none_or(|c| c.is_ascii_alphabetic() || c == '_')
                && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
            if !valid {
                return Err(Error::Unsupported(format!(
                    "`{label}` is not a label name; use letters, digits and _, like exported_namespace"
                )));
            }
        }
        Ok(self)
    }
}

/// The most recent contexts kept connected when they are not the current one.
pub const MAX_KEEP_RECENT: u32 = 5;

/// How Kubri keeps connections to contexts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConnectionSettings {
    /// How many recently used contexts stay connected in the background, so switching back to
    /// one is instant. 0 turns it off; at most [`MAX_KEEP_RECENT`].
    pub keep_recent: u32,
}

impl Default for ConnectionSettings {
    fn default() -> Self {
        Self { keep_recent: 3 }
    }
}

/// A search kept in the sidebar.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SavedSearch {
    pub name: String,
    /// As typed in the search field, like `web kind:deploy,svc -l tier=front`.
    pub query: String,
}

impl Settings {
    pub fn context(&self, name: &str) -> ContextSettings {
        self.contexts.get(name).cloned().unwrap_or_default()
    }

    /// Sets how lists show a kind; the defaults remove its entry.
    pub(crate) fn set_columns(&mut self, kind: &str, columns: ColumnSettings) -> Result<()> {
        let columns = columns.normalized()?;
        if columns.is_default() {
            self.columns.remove(kind);
        } else {
            self.columns.insert(kind.to_owned(), columns);
        }
        Ok(())
    }

    /// Sets a context's settings; the defaults remove its entry.
    pub(crate) fn set_context(&mut self, name: &str, settings: ContextSettings) -> Result<()> {
        let settings = settings.normalized()?;
        if settings.is_default() {
            self.contexts.remove(name);
        } else {
            self.contexts.insert(name.to_owned(), settings);
        }
        Ok(())
    }
}

/// The settings file, or `None` where there is no config folder.
pub(crate) fn path() -> Option<PathBuf> {
    Some(dir()?.join("settings.json"))
}

/// Kubri's config folder: `$KUBRI_CONFIG_DIR`, or `kubri` in the system's config folder.
pub fn dir() -> Option<PathBuf> {
    let var = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let dir = var("KUBRI_CONFIG_DIR").or_else(|| {
        var("XDG_CONFIG_HOME")
            .or_else(dirs::config_local_dir)
            .map(|d| d.join("kubri"))
    })?;
    Some(dir)
}

/// Reads the settings; a missing file gives the defaults, a broken one an error to show.
pub(crate) fn read(path: &Path) -> Result<Settings, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(err) => Err(format!("{}: {err}", path.display())),
    }
}

/// Writes the settings through a temporary file, so a crash never leaves half a file.
pub(crate) fn write(path: &Path, settings: &Settings) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(settings)?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, text + "\n")?;
    std::fs::rename(&temp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_leave_no_entry() {
        let mut settings = Settings::default();
        let prod = ContextSettings {
            production: true,
            color: " #D64545 ".into(),
            ..Default::default()
        };
        settings.set_context("prod", prod).unwrap();
        assert_eq!(settings.context("prod").color, "#d64545");
        settings.set_context("prod", ContextSettings::default()).unwrap();
        assert!(settings.contexts.is_empty());
    }

    #[test]
    fn patterns_match_names() {
        for (pattern, name, yes) in [
            ("*-prod", "shop-prod", true),
            ("*-prod", "shop-prod-2", false),
            ("prod-*", "prod-eu", true),
            ("prod-*", "prod-", true),
            ("*prod*", "payments-production", true),
            ("a*b*c", "abc", true),
            ("a*b*c", "acb", false),
            ("ab*ba", "aba", false),
            ("shop", "shop", true),
            ("shop", "shops", false),
            ("*", "anything", true),
        ] {
            assert_eq!(name_matches(pattern, name), yes, "{pattern} {name}");
        }
    }

    #[test]
    fn selectors_match_labels() {
        let labels: BTreeMap<String, String> = [
            ("env".into(), "production".into()),
            ("team".into(), "shop".into()),
        ]
        .into();
        for (selector, yes) in [
            ("env=production", true),
            ("env == production", true),
            ("env=staging", false),
            ("env!=staging", true),
            ("tier!=web", true),
            ("team", true),
            ("!team", false),
            ("!canary", true),
            ("env=production,team=web", false),
            ("env=production, team", true),
            ("env in (production)", false),
            ("", false),
        ] {
            assert_eq!(labels_match(selector, &labels), yes, "{selector}");
        }
    }

    #[test]
    fn production_rules_are_kept_tidy() {
        let mut settings = Settings::default();
        let rules = ContextSettings {
            production_namespaces: vec!["shop".into(), "*-prod".into()],
            production_labels: vec![" env = production ".into(), "env=production".into(), " ".into()],
            ..Default::default()
        };
        settings.set_context("dev", rules).unwrap();
        let dev = settings.context("dev");
        assert_eq!(dev.production_labels, ["env=production"]);
        assert_eq!(dev.production_by_name("shop"), Some("shop"));
        assert_eq!(dev.production_by_name("shop-prod"), Some("*-prod"));
        assert_eq!(dev.production_by_name("web"), None);
        let prod: BTreeMap<String, String> = [("env".into(), "production".into())].into();
        assert_eq!(dev.production_by_labels(Some(&prod)), Some("env=production"));
        assert_eq!(dev.production_by_labels(Some(&BTreeMap::new())), None);
        assert_eq!(dev.production_by_labels(None), Some("env=production"));
        let bad = ContextSettings {
            production_labels: vec!["env in (prod)".into()],
            ..Default::default()
        };
        let err = settings.set_context("dev", bad).unwrap_err().to_string();
        assert!(err.contains("env in (prod)"), "{err}");
    }

    #[test]
    fn bad_colors_are_refused() {
        let mut settings = Settings::default();
        for color in ["red", "#12345", "#12345g", "123456"] {
            let s = ContextSettings {
                color: color.into(),
                ..Default::default()
            };
            assert!(settings.set_context("x", s).is_err(), "{color}");
        }
    }

    #[test]
    fn round_trips_through_the_file() {
        let dir = std::env::temp_dir().join(format!("kubri-settings-{}", std::process::id()));
        let path = dir.join("settings.json");
        assert_eq!(read(&path).unwrap(), Settings::default());
        let mut settings = Settings::default();
        settings
            .set_context(
                "prod",
                ContextSettings {
                    display_name: "Production".into(),
                    locked: true,
                    ..Default::default()
                },
            )
            .unwrap();
        settings.diff.scale = true;
        settings
            .set_columns(
                "apps/Deployment",
                ColumnSettings {
                    hidden: vec!["Up-to-date".into()],
                    custom: vec![CustomColumn {
                        name: " ".into(),
                        path: " .spec.replicas ".into(),
                    }],
                    ..Default::default()
                },
            )
            .unwrap();
        // A column without a name is named by its path.
        assert_eq!(
            settings.columns["apps/Deployment"].custom[0].name,
            ".spec.replicas"
        );
        assert!(
            settings
                .set_columns(
                    "/Pod",
                    ColumnSettings {
                        custom: vec![CustomColumn {
                            name: "x".into(),
                            path: ".a[".into(),
                        }],
                        ..Default::default()
                    },
                )
                .is_err()
        );
        settings
            .set_columns("apps/Deployment", ColumnSettings::default())
            .unwrap();
        assert!(settings.columns.is_empty());
        let favorite: FavoriteForward = serde_json::from_value(serde_json::json!({
            "target": { "resource": { "group": "", "version": "v1", "plural": "services" }, "namespace": "shop", "name": "web" },
            "remotePort": 80, "localPort": 8080
        }))
        .unwrap();
        assert_eq!(favorite.address, "");
        assert!(favorite.start_on_connect);
        let mut prod = settings.context("prod");
        prod.forwards.push(favorite);
        settings.set_context("prod", prod).unwrap();
        write(&path, &settings).unwrap();
        assert_eq!(read(&path).unwrap(), settings);
        // Unknown and missing fields are fine, so older and newer versions share the file.
        std::fs::write(&path, r#"{"contexts":{"a":{"production":true,"future":1}}}"#).unwrap();
        let read_back = read(&path).unwrap();
        assert!(read_back.context("a").production);
        assert_eq!(read_back.diff, DiffSettings::default());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
