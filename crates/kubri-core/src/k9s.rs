//! Reads the k9s configuration: aliases, hotkeys, plugins and skins, from the same files k9s
//! uses, so a k9s setup works in Kubri as it is.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Error, Result};

/// Everything Kubri takes from the k9s configuration, for one context.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct K9sConfig {
    /// Where the files were looked for: the config directory, then the data directory.
    pub dirs: Vec<String>,
    /// The files that were read, and why one could not be.
    pub files: Vec<ConfigFile>,
    /// Sorted by name.
    pub aliases: Vec<Alias>,
    pub hotkeys: Vec<Hotkey>,
    pub plugins: Vec<Plugin>,
    /// Skins in the skins directories, by name.
    pub skins: Vec<String>,
    /// The skin k9s uses for the context: `K9S_SKIN`, the context's, or the default one.
    pub skin: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFile {
    pub path: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Alias {
    pub name: String,
    /// What it stands for, such as `v1/pods` or `pod kube-system app=dns`.
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hotkey {
    #[serde(skip_deserializing)]
    pub name: String,
    #[serde(default)]
    pub short_cut: String,
    #[serde(default)]
    pub description: String,
    /// A command as typed after `:`, which may use the plugin variables.
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub keep_history: bool,
    /// Takes the key even where Kubri uses it.
    #[serde(default, rename = "override")]
    pub override_: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plugin {
    #[serde(skip_deserializing)]
    pub name: String,
    #[serde(default)]
    pub short_cut: String,
    #[serde(default)]
    pub description: String,
    /// Resource names the plugin is offered for, or `all`.
    #[serde(default, deserialize_with = "strings")]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub command: String,
    #[serde(default, deserialize_with = "strings")]
    pub args: Vec<String>,
    /// Runs without a terminal, and Kubri does not wait for it.
    #[serde(default)]
    pub background: bool,
    /// Shows the command and asks before running it.
    #[serde(default)]
    pub confirm: bool,
    #[serde(default)]
    pub dangerous: bool,
    #[serde(default, rename = "override")]
    pub override_: bool,
}

/// Scalars as strings: YAML has `- 8080` as a number, but it is an argument like any other.
fn strings<'de, D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Vec<String>, D::Error> {
    let values = Option::<Vec<Value>>::deserialize(deserializer)?.unwrap_or_default();
    values
        .into_iter()
        .map(|v| match v {
            Value::String(s) => Ok(s),
            Value::Number(n) => Ok(n.to_string()),
            Value::Bool(b) => Ok(b.to_string()),
            other => Err(de::Error::custom(format!("expected a string, got {other}"))),
        })
        .collect()
}

/// Where k9s keeps its files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Dirs {
    pub config: PathBuf,
    pub data: PathBuf,
}

impl Dirs {
    /// The directories k9s uses: `K9S_CONFIG_DIR` for both, or the XDG config and data
    /// directories (`~/.config/k9s` and `~/.local/share/k9s` on Linux, `~/Library/Application
    /// Support/k9s` on macOS, `%LOCALAPPDATA%\k9s` on Windows).
    pub fn from_env() -> Option<Self> {
        let var = |name: &str| {
            std::env::var_os(name)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        if let Some(dir) = var("K9S_CONFIG_DIR") {
            return Some(Self {
                config: dir.clone(),
                data: dir,
            });
        }
        let config = var("XDG_CONFIG_HOME")
            .or_else(dirs::config_local_dir)?
            .join("k9s");
        let data = var("XDG_DATA_HOME").or_else(dirs::data_local_dir)?.join("k9s");
        Some(Self { config, data })
    }

    /// Lowest priority first, so later entries override earlier ones.
    fn roots(&self) -> Vec<&Path> {
        if self.data == self.config {
            vec![&self.config]
        } else {
            vec![&self.data, &self.config]
        }
    }

    fn context_dir(&self, context: &ContextKey) -> Option<PathBuf> {
        let cluster = path_component(&context.cluster)?;
        let name = path_component(&context.context)?;
        Some(self.data.join("clusters").join(cluster).join(name))
    }
}

/// The context k9s keeps per-context files for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContextKey {
    pub cluster: String,
    pub context: String,
}

/// A cluster or context name as k9s puts it in a path: each run of `:` and `/` becomes one
/// `-`. Names that would leave the directory are refused.
fn path_component(name: &str) -> Option<String> {
    let mut out = String::with_capacity(name.len());
    let mut in_run = false;
    for c in name.chars() {
        let separator = matches!(c, ':' | '/' | '\\');
        if !separator {
            out.push(c);
        } else if !in_run {
            out.push('-');
        }
        in_run = separator;
    }
    (!out.is_empty() && out != "." && out != "..").then_some(out)
}

/// Collects the files that were read and their errors.
#[derive(Default)]
struct Reader {
    files: Vec<ConfigFile>,
}

impl Reader {
    /// Parses a YAML file, or `None` if it does not exist or is broken (which is recorded).
    fn read<T: de::DeserializeOwned>(&mut self, path: &Path) -> Option<T> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
            Err(err) => {
                self.note(path, Some(err.to_string()));
                return None;
            }
        };
        match parse(&text) {
            Ok(value) => {
                self.note(path, None);
                Some(value)
            }
            Err(err) => {
                self.note(path, Some(err));
                None
            }
        }
    }

    fn note(&mut self, path: &Path, error: Option<String>) {
        self.files.push(ConfigFile {
            path: path.display().to_string(),
            error,
        });
    }
}

fn parse<T: de::DeserializeOwned>(text: &str) -> std::result::Result<T, String> {
    if text.trim().is_empty() {
        // An empty file is a file with nothing in it, not an error.
        return serde_json::from_value(Value::Object(Default::default())).map_err(|e| e.to_string());
    }
    serde_saphyr::from_str(text).map_err(|e| e.to_string())
}

#[derive(Deserialize, Default)]
struct AliasFile {
    #[serde(default)]
    aliases: BTreeMap<String, String>,
}

#[derive(Deserialize, Default)]
struct HotkeyFile {
    #[serde(default, rename = "hotKeys")]
    hot_keys: BTreeMap<String, Hotkey>,
}

#[derive(Deserialize, Default)]
struct PluginFile {
    #[serde(default)]
    plugins: Option<BTreeMap<String, Plugin>>,
    /// Files in the plugins directory may also hold plugins at the top level.
    #[serde(flatten)]
    rest: BTreeMap<String, Value>,
}

impl PluginFile {
    fn into_plugins(self) -> std::result::Result<BTreeMap<String, Plugin>, String> {
        if let Some(plugins) = self.plugins {
            return Ok(plugins);
        }
        self.rest
            .into_iter()
            .map(|(name, value)| {
                serde_json::from_value(value)
                    .map(|p| (name.clone(), p))
                    .map_err(|e| format!("plugin {name}: {e}"))
            })
            .collect()
    }
}

#[derive(Deserialize, Default)]
struct RootConfig {
    #[serde(default)]
    k9s: RootK9s,
}

#[derive(Deserialize, Default)]
struct RootK9s {
    #[serde(default)]
    ui: RootUi,
}

#[derive(Deserialize, Default)]
struct RootUi {
    #[serde(default)]
    skin: Option<String>,
}

#[derive(Deserialize, Default)]
struct ContextConfig {
    #[serde(default)]
    k9s: ContextK9s,
}

#[derive(Deserialize, Default)]
struct ContextK9s {
    #[serde(default)]
    skin: Option<String>,
}

/// Reads the k9s configuration for `context`. Missing files are fine; broken ones are listed
/// with their error in [`K9sConfig::files`].
pub(crate) fn load(dirs: &Dirs, context: Option<&ContextKey>, env_skin: Option<String>) -> K9sConfig {
    let mut reader = Reader::default();
    let roots = dirs.roots();
    let context_dir = context.and_then(|c| dirs.context_dir(c));
    // Global files first, the context's last, so it wins.
    let mut places: Vec<PathBuf> = roots.iter().map(|r| r.to_path_buf()).collect();
    places.extend(context_dir.clone());

    let mut aliases = BTreeMap::new();
    let mut hotkeys = BTreeMap::new();
    let mut plugins = BTreeMap::new();
    for (i, place) in places.iter().enumerate() {
        if let Some(file) = reader.read::<AliasFile>(&place.join("aliases.yaml")) {
            aliases.extend(file.aliases);
        }
        if let Some(file) = reader.read::<HotkeyFile>(&place.join("hotkeys.yaml")) {
            hotkeys.extend(file.hot_keys);
        }
        let mut plugin_files = vec![place.join("plugins.yaml")];
        // The plugins directory holds one or more plugins per file; not per context.
        if i < roots.len() {
            plugin_files.extend(yaml_files(&place.join("plugins")));
        }
        for path in plugin_files {
            if let Some(file) = reader.read::<PluginFile>(&path) {
                match file.into_plugins() {
                    Ok(found) => plugins.extend(found),
                    Err(err) => {
                        if let Some(last) = reader.files.last_mut() {
                            last.error = Some(err);
                        }
                    }
                }
            }
        }
    }

    let global_skin = reader
        .read::<RootConfig>(&dirs.config.join("config.yaml"))
        .and_then(|c| c.k9s.ui.skin);
    let context_skin = context_dir
        .as_ref()
        .and_then(|dir| reader.read::<ContextConfig>(&dir.join("config.yaml")))
        .and_then(|c| c.k9s.skin);
    let skin = [env_skin, context_skin, global_skin]
        .into_iter()
        .flatten()
        .map(|s| s.trim().to_owned())
        .find(|s| !s.is_empty());

    let mut skins: Vec<String> = roots
        .iter()
        .flat_map(|r| yaml_files(&r.join("skins")))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    skins.sort();
    skins.dedup();

    K9sConfig {
        dirs: roots.iter().rev().map(|r| r.display().to_string()).collect(),
        files: reader.files,
        aliases: aliases
            .into_iter()
            .map(|(name, command)| Alias { name, command })
            .collect(),
        hotkeys: hotkeys
            .into_iter()
            .map(|(name, hotkey)| Hotkey { name, ..hotkey })
            .collect(),
        plugins: plugins
            .into_iter()
            .map(|(name, plugin)| Plugin { name, ..plugin })
            .collect(),
        skins,
        skin,
    }
}

/// The `.yaml` and `.yml` files of a directory, by name.
fn yaml_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("yaml") || e.eq_ignore_ascii_case("yml"))
        })
        .collect();
    files.sort();
    files
}

/// A skin file as JSON, with YAML anchors resolved. The UI turns its colors into a theme.
pub(crate) fn skin(dirs: &Dirs, name: &str) -> Result<Value> {
    let valid = !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if !valid {
        return Err(Error::Unsupported(format!("`{name}` is not a skin name")));
    }
    let candidates = dirs
        .roots()
        .into_iter()
        .rev()
        .flat_map(|root| ["yaml", "yml"].map(|ext| root.join("skins").join(format!("{name}.{ext}"))));
    for path in candidates {
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                return parse::<Value>(&text)
                    .map_err(|e| Error::Unsupported(format!("cannot read {}: {e}", path.display())));
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(Error::Io(err)),
        }
    }
    Err(Error::NotFound(format!("skin {name}")))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A fresh directory for one test.
    pub(crate) fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kubri-k9s-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    pub(crate) fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn reads_aliases_hotkeys_and_plugins_with_context_overrides() {
        let root = scratch("load");
        let dirs = Dirs {
            config: root.join("config"),
            data: root.join("data"),
        };
        write(
            &dirs.config.join("aliases.yaml"),
            "aliases:\n  pp: v1/pods\n  fred: pod fred app=blee\n",
        );
        write(
            &dirs.config.join("hotkeys.yaml"),
            "hotKeys:\n  shift-0:\n    shortCut: Shift-0\n    description: Viewing pods\n    command: pods\n",
        );
        write(
            &dirs.config.join("plugins.yaml"),
            r#"plugins:
  fred:
    shortCut: Ctrl-L
    description: Pod logs
    scopes: [pods]
    command: kubectl
    args: [logs, -f, $NAME, -n, $NAMESPACE, --context, $CONTEXT]
"#,
        );
        // A plugins directory file with plugins at the top level and a numeric argument.
        write(
            &dirs.data.join("plugins").join("forward.yml"),
            "pf:\n  shortCut: Shift-P\n  scopes: [svc]\n  command: kubectl\n  background: true\n  args: [port-forward, $NAME, 8080]\n",
        );
        write(
            &dirs.config.join("config.yaml"),
            "k9s:\n  ui:\n    skin: dracula\n",
        );
        write(&dirs.config.join("skins").join("dracula.yaml"), "k9s: {}\n");
        write(&dirs.data.join("skins").join("in-the-navy.yml"), "k9s: {}\n");
        let context_dir = dirs.data.join("clusters").join("prod-eu").join("arn-aws-prod");
        write(
            &context_dir.join("aliases.yaml"),
            "aliases:\n  pp: apps/v1/deployments\n",
        );
        write(&context_dir.join("config.yaml"), "k9s:\n  skin: in-the-navy\n");
        write(&context_dir.join("hotkeys.yaml"), "hotKeys: [broken\n");

        let context = ContextKey {
            cluster: "prod-eu".into(),
            context: "arn:aws:prod".into(),
        };
        let config = load(&dirs, Some(&context), None);
        assert_eq!(
            config.aliases,
            vec![
                Alias {
                    name: "fred".into(),
                    command: "pod fred app=blee".into()
                },
                Alias {
                    name: "pp".into(),
                    command: "apps/v1/deployments".into()
                },
            ]
        );
        assert_eq!(config.hotkeys.len(), 1);
        assert_eq!(config.hotkeys[0].name, "shift-0");
        assert_eq!(config.hotkeys[0].short_cut, "Shift-0");
        let names: Vec<&str> = config.plugins.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["fred", "pf"]);
        assert_eq!(config.plugins[0].args[2], "$NAME");
        assert_eq!(config.plugins[1].args[2], "8080");
        assert!(config.plugins[1].background);
        assert_eq!(config.skins, ["dracula", "in-the-navy"]);
        assert_eq!(config.skin.as_deref(), Some("in-the-navy"));
        let broken: Vec<&ConfigFile> = config.files.iter().filter(|f| f.error.is_some()).collect();
        assert_eq!(broken.len(), 1);
        assert!(broken[0].path.ends_with("hotkeys.yaml"), "{broken:?}");

        // Without the context, the global skin; K9S_SKIN beats both.
        assert_eq!(load(&dirs, None, None).skin.as_deref(), Some("dracula"));
        assert_eq!(
            load(&dirs, Some(&context), Some("blue".into())).skin.as_deref(),
            Some("blue")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_directories_are_empty() {
        let root = scratch("empty");
        let dirs = Dirs {
            config: root.join("nothing"),
            data: root.join("nothing"),
        };
        let config = load(&dirs, None, None);
        assert_eq!(config.dirs.len(), 1);
        assert!(config.files.is_empty() && config.plugins.is_empty() && config.skin.is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reads_skins_with_anchors() {
        let root = scratch("skin");
        let dirs = Dirs {
            config: root.clone(),
            data: root.clone(),
        };
        write(
            &root.join("skins").join("dracula.yaml"),
            "foreground: &fg \"#f8f8f2\"\nk9s:\n  body:\n    fgColor: *fg\n",
        );
        let skin = skin(&dirs, "dracula").unwrap();
        assert_eq!(skin["k9s"]["body"]["fgColor"], "#f8f8f2");
        assert!(matches!(
            super::skin(&dirs, "../secrets"),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(super::skin(&dirs, "nope"), Err(Error::NotFound(_))));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn context_names_become_safe_path_components() {
        assert_eq!(
            path_component("arn:aws:eks:eu-west-1:1234:cluster/prod").unwrap(),
            "arn-aws-eks-eu-west-1-1234-cluster-prod"
        );
        assert_eq!(
            path_component("gke_proj_zone_name").unwrap(),
            "gke_proj_zone_name"
        );
        assert_eq!(
            path_component("https://api.example:6443").unwrap(),
            "https-api.example-6443"
        );
        assert_eq!(path_component(".."), None);
        assert_eq!(path_component(""), None);
    }
}
