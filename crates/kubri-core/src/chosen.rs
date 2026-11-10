//! Kubeconfig files the user chose in Kubri. Unlike added contexts, which are copied into
//! Kubri's own folder, these are read where they are, every time Kubri reads the kubeconfig:
//! changes to them show up on the next reload, and paths written relative to them work.
//! They come after the files in `KUBECONFIG`, so a context there wins over one of the same
//! name in a chosen file, as an earlier file in `KUBECONFIG` wins over a later one.

use std::path::{Path, PathBuf};

use kube::config::Kubeconfig;
use serde::Serialize;

use crate::{Error, Result};

const LIST: &str = "kubeconfig-files.json";

/// A kubeconfig file Kubri reads, with what it found in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct KubeconfigFile {
    pub path: String,
    /// Chosen in Kubri, so it can be removed there; otherwise from `KUBECONFIG` or the default.
    pub chosen: bool,
    pub contexts: Vec<String>,
    /// Contexts an earlier file already has, so this file's are not used.
    pub shadowed: Vec<String>,
    /// It does not exist, so it is skipped, as kubectl skips it.
    pub missing: bool,
    /// Why Kubri cannot read it.
    pub problem: Option<String>,
}

/// The chosen files, in the order they were chosen.
pub(crate) fn files() -> Vec<PathBuf> {
    crate::config_dir()
        .map(|d| read_list(&d.join(LIST)))
        .unwrap_or_default()
}

fn read_list(path: &Path) -> Vec<PathBuf> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<PathBuf>>(&bytes).ok())
        .unwrap_or_default()
}

fn write_list(path: &Path, files: &[PathBuf]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, serde_json::to_string_pretty(files)? + "\n")?;
    std::fs::rename(&temp, path)?;
    Ok(())
}

fn list_path() -> Result<PathBuf> {
    crate::config_dir()
        .map(|d| d.join(LIST))
        .ok_or_else(|| Error::Unsupported("Kubri has no config folder to remember files in".into()))
}

/// `~/…` in the user's home folder; other paths as they are.
fn expand(path: &str) -> PathBuf {
    let path = path.trim();
    match path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        Some(rest) => dirs::home_dir()
            .map(|h| h.join(rest))
            .unwrap_or_else(|| path.into()),
        None if path == "~" => dirs::home_dir().unwrap_or_else(|| path.into()),
        None => path.into(),
    }
}

/// Checks that `path` is a kubeconfig file Kubri can read; returns it with `~` expanded.
fn check(path: &str) -> Result<PathBuf> {
    let full = expand(path);
    if !full.is_absolute() {
        return Err(Error::Unsupported(format!(
            "{path}: give the full path of the file"
        )));
    }
    if !full.exists() {
        return Err(Error::Unsupported(format!("{path} does not exist")));
    }
    if full.is_dir() {
        return Err(Error::Unsupported(format!(
            "{path} is a folder, not a kubeconfig file"
        )));
    }
    Kubeconfig::read_from(&full)
        .map_err(|e| Error::Unsupported(format!("{path} is not a kubeconfig Kubri can read: {e}")))?;
    Ok(full)
}

/// Adds `paths` to `list`, skipping ones in `own` or already there; returns how many were new.
fn add_to(list: &Path, own: &[PathBuf], paths: &[String]) -> Result<usize> {
    let checked = paths.iter().map(|p| check(p)).collect::<Result<Vec<_>>>()?;
    let mut files = read_list(list);
    let before = files.len();
    for path in checked {
        if !own.contains(&path) && !files.contains(&path) {
            files.push(path);
        }
    }
    let added = files.len() - before;
    if added > 0 {
        write_list(list, &files)?;
    }
    Ok(added)
}

/// Remembers `paths` as chosen files; ones already read are skipped. Each must be a
/// kubeconfig Kubri can read, or none is added.
pub(crate) fn add(own: &[PathBuf], paths: &[String]) -> Result<usize> {
    add_to(&list_path()?, own, paths)
}

/// Forgets the chosen file `path`; false when it was not chosen in Kubri.
pub(crate) fn remove(path: &str) -> Result<bool> {
    remove_from(&list_path()?, path)
}

fn remove_from(list: &Path, path: &str) -> Result<bool> {
    let mut files = read_list(list);
    let before = files.len();
    let path = expand(path);
    files.retain(|f| f != &path);
    if files.len() == before {
        return Ok(false);
    }
    write_list(list, &files)?;
    Ok(true)
}

/// Every kubeconfig file Kubri reads, own files first, with their contexts.
pub(crate) fn describe(own: &[PathBuf], chosen: &[PathBuf]) -> Vec<KubeconfigFile> {
    let mut seen = std::collections::HashSet::new();
    own.iter()
        .map(|p| (p, false))
        .chain(chosen.iter().filter(|p| !own.contains(p)).map(|p| (p, true)))
        .map(|(path, chosen)| {
            let mut missing = false;
            let (contexts, problem) = if path.is_dir() {
                (Vec::new(), Some("It is a folder, not a file".to_owned()))
            } else {
                match Kubeconfig::read_from(path) {
                    Ok(k) => (k.contexts.into_iter().map(|c| c.name).collect(), None),
                    Err(kube::config::KubeconfigError::ReadConfig(err, _))
                        if err.kind() == std::io::ErrorKind::NotFound =>
                    {
                        missing = true;
                        (Vec::new(), None)
                    }
                    Err(err) => (Vec::new(), Some(err.to_string())),
                }
            };
            let shadowed = contexts
                .iter()
                .filter(|c| !seen.insert((*c).clone()))
                .cloned()
                .collect();
            KubeconfigFile {
                path: path.display().to_string(),
                chosen,
                contexts,
                shadowed,
                missing,
                problem,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::k9s::tests::scratch;

    fn kubeconfig(dir: &Path, name: &str, contexts: &[&str]) -> PathBuf {
        let path = dir.join(name);
        let entries: Vec<String> = contexts
            .iter()
            .map(|c| format!("- {{name: {c}, context: {{cluster: c}}}}"))
            .collect();
        std::fs::write(&path, format!("contexts:\n{}\n", entries.join("\n"))).unwrap();
        path
    }

    #[test]
    fn remembers_chosen_files_once() {
        let dir = scratch("add");
        let list = dir.join(LIST);
        let own = kubeconfig(&dir, "own.yaml", &["a"]);
        let extra = kubeconfig(&dir, "extra.yaml", &["b"]);
        let as_text = |p: &Path| p.display().to_string();

        assert_eq!(
            add_to(
                &list,
                std::slice::from_ref(&own),
                &[as_text(&extra), as_text(&own)]
            )
            .unwrap(),
            1
        );
        assert_eq!(add_to(&list, &[], &[as_text(&extra)]).unwrap(), 0);
        assert_eq!(read_list(&list), std::slice::from_ref(&extra));

        // One bad file and nothing is added.
        let broken = dir.join("broken.yaml");
        std::fs::write(&broken, "contexts: [ {").unwrap();
        let other = kubeconfig(&dir, "other.yaml", &["c"]);
        assert!(add_to(&list, &[], &[as_text(&other), as_text(&broken)]).is_err());
        assert!(
            add_to(&list, &[], &[as_text(&dir)])
                .unwrap_err()
                .to_string()
                .contains("is a folder")
        );
        assert!(add_to(&list, &[], &["relative.yaml".into()]).is_err());
        assert!(add_to(&list, &[], &[as_text(&dir.join("missing.yaml"))]).is_err());
        assert_eq!(read_list(&list), std::slice::from_ref(&extra));

        assert!(remove_from(&list, &as_text(&extra)).unwrap());
        assert!(!remove_from(&list, &as_text(&extra)).unwrap());
        assert!(read_list(&list).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn describes_every_file_read() {
        let dir = scratch("describe");
        let own = kubeconfig(&dir, "own.yaml", &["a", "b"]);
        let missing = dir.join("missing.yaml");
        let extra = kubeconfig(&dir, "extra.yaml", &["b", "c"]);
        let files = describe(&[own.clone(), missing], &[extra, own]);
        assert_eq!(files.len(), 3);
        assert!(!files[0].chosen && files[0].contexts == ["a", "b"] && files[0].problem.is_none());
        assert!(files[1].missing && files[1].problem.is_none());
        assert!(files[2].chosen);
        assert_eq!(files[2].shadowed, ["b"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn expands_the_home_folder() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(expand("~/.kube/lab.yaml"), home.join(".kube/lab.yaml"));
        assert_eq!(expand(" /etc/k.yaml "), PathBuf::from("/etc/k.yaml"));
    }
}
