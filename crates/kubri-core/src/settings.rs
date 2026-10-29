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
    /// Destructive actions ask to type the object's name, and a banner says where you are.
    pub production: bool,
    /// Kubri never changes this cluster until this is turned off again.
    pub read_only: bool,
    /// Starts read-only; unlocking lasts 15 minutes at a time.
    pub locked: bool,
    /// The namespace to show after connecting; empty for all namespaces.
    pub namespace: String,
    /// AI agents connected over MCP may change this context, each change confirmed in Kubri.
    /// Production contexts never allow it.
    pub agent_writes: bool,
    /// Port-forwards that start when Kubri connects to this context.
    pub forwards: Vec<FavoriteForward>,
}

/// A port-forward to start whenever Kubri connects to its context, on the same local port.
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
    fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Trims what was typed and refuses colors the UI could not use.
    fn normalized(mut self) -> Result<Self> {
        self.display_name = self.display_name.trim().to_owned();
        self.label = self.label.trim().to_owned();
        self.namespace = self.namespace.trim().to_owned();
        self.color = self.color.trim().to_ascii_lowercase();
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
