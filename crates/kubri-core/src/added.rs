//! Contexts added in Kubri from a kubeconfig the user picks, kept apart from their own
//! kubeconfig files, which Kubri never changes.
//!
//! Each added context is one file in `kubeconfigs` in Kubri's config folder, with its cluster
//! and user renamed after the context so they cannot clash with entries of the same name in
//! other files. They are merged after the files in `KUBECONFIG`, and handed to helm and k9s
//! plugins the same way.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use kube::config::{Kubeconfig, NamedAuthInfo, NamedCluster, NamedContext};
use serde::Serialize;

use crate::{Error, Result};

/// One context of a kubeconfig about to be added.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NewContext {
    pub name: String,
    pub server: Option<String>,
    pub user: Option<String>,
    pub namespace: Option<String>,
    /// The command it runs to sign in, like `aws` or `gke-gcloud-auth-plugin`.
    pub exec: Option<String>,
    /// Why it cannot be added; `None` when it can.
    pub problem: Option<String>,
}

/// The folder of added contexts.
pub(crate) fn dir() -> Option<PathBuf> {
    crate::config_dir().map(|d| d.join("kubeconfigs"))
}

/// The files of added contexts, in a stable order.
pub(crate) fn files() -> Vec<PathBuf> {
    dir().map(|d| files_in(&d)).unwrap_or_default()
}

fn files_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "yaml"))
        .collect();
    files.sort();
    files
}

/// The names of the contexts added in Kubri.
pub(crate) fn names() -> HashSet<String> {
    files()
        .iter()
        .filter_map(|f| Kubeconfig::read_from(f).ok())
        .flat_map(|k| k.contexts.into_iter().map(|c| c.name))
        .collect()
}

/// Reads pasted or dropped kubeconfig text; several documents are merged.
pub(crate) fn parse(text: &str) -> Result<Kubeconfig> {
    Kubeconfig::from_yaml(text).map_err(|e| Error::InvalidKubeconfig(e.to_string()))
}

/// A path written relative to the kubeconfig it came from, which Kubri cannot resolve.
fn relative(path: &Option<String>) -> Option<&str> {
    path.as_deref()
        .filter(|p| !p.is_empty() && !Path::new(p).is_absolute())
}

/// A path to a file, absolute or not.
fn named(path: &Option<String>) -> Option<&str> {
    path.as_deref().filter(|p| !p.is_empty())
}

/// The contexts of `new`, and why any of them cannot be added next to `existing`.
///
/// `local` says whether a context may run commands and read files on the machine Kubri runs
/// on. On a web server that machine is the server, and everyone who signs in could use a
/// context they add to run commands there, or to send a file from there to a server of their
/// choice as a token. So the server allows it only where it allows local terminals.
pub(crate) fn inspect(new: &Kubeconfig, existing: &HashSet<String>, local: bool) -> Vec<NewContext> {
    new.contexts
        .iter()
        .map(|named_context| {
            let context = named_context.context.as_ref();
            let cluster_name = context.map(|c| c.cluster.clone()).unwrap_or_default();
            let user_name = context.and_then(|c| c.user.clone());
            let cluster = new
                .clusters
                .iter()
                .find(|c| c.name == cluster_name)
                .and_then(|c| c.cluster.as_ref());
            let user = user_name
                .as_ref()
                .and_then(|u| new.auth_infos.iter().find(|a| &a.name == u))
                .and_then(|a| a.auth_info.as_ref());
            // Each file a context can name, with the field that holds the content instead.
            let files = [
                (cluster.map(|c| &c.certificate_authority), "certificate-authority-data"),
                (user.map(|u| &u.client_certificate), "client-certificate-data"),
                (user.map(|u| &u.client_key), "client-key-data"),
                (user.map(|u| &u.token_file), "token"),
            ];
            // An exec plugin, or the gcp auth provider's `cmd-path`.
            let command = user.and_then(|u| u.exec.as_ref()).and_then(|e| e.command.clone()).or_else(|| {
                user.and_then(|u| u.auth_provider.as_ref())
                    .and_then(|p| p.config.get("cmd-path").cloned())
            });
            let problem = if existing.contains(&named_context.name) {
                Some("A context with this name is already there".to_owned())
            } else if cluster.is_none() {
                Some(format!("Its cluster `{cluster_name}` is not in the file"))
            } else if user_name.is_some() && user.is_none() {
                Some(format!("Its user `{}` is not in the file", user_name.clone().unwrap_or_default()))
            } else if !local && let Some(command) = &command {
                Some(format!(
                    "It signs in by running `{command}` on the server, which the server allows only when started with KUBRI_LOCAL_TERMINAL=true"
                ))
            } else if !local {
                files.iter().find_map(|&(path, data)| {
                    let file = named(path?)?;
                    Some(format!(
                        "It reads the file `{file}` on the server; put the file's content in `{data}` instead"
                    ))
                })
            } else {
                files.iter().find_map(|&(path, _)| relative(path?)).map(|file| {
                    format!(
                        "It names the file `{file}` relative to where the kubeconfig was; add the kubeconfig to KUBECONFIG instead, or use an absolute path"
                    )
                })
            };
            NewContext {
                name: named_context.name.clone(),
                server: cluster.and_then(|c| c.server.clone()),
                user: user_name,
                namespace: context.and_then(|c| c.namespace.clone()),
                exec: command,
                problem,
            }
        })
        .collect()
}

/// `name` as a file name: letters, digits, dots, dashes and underscores.
fn file_stem(name: &str) -> String {
    let stem: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    stem.trim_start_matches('.').chars().take(80).collect()
}

/// The kubeconfig with only `context`, its cluster and user renamed after it.
fn single(new: &Kubeconfig, context: &str) -> Result<Kubeconfig> {
    let named = new
        .contexts
        .iter()
        .find(|c| c.name == context)
        .ok_or_else(|| Error::UnknownContext(context.to_owned()))?;
    let mut entry = named.context.clone().unwrap_or_default();
    let own = format!("kubri-{context}");
    let cluster = new
        .clusters
        .iter()
        .find(|c| c.name == entry.cluster)
        .map(|c| NamedCluster {
            name: own.clone(),
            ..c.clone()
        });
    let user = entry
        .user
        .as_ref()
        .and_then(|u| new.auth_infos.iter().find(|a| &a.name == u))
        .map(|a| NamedAuthInfo {
            name: own.clone(),
            ..a.clone()
        });
    entry.cluster = own.clone();
    if user.is_some() {
        entry.user = Some(own);
    }
    Ok(Kubeconfig {
        api_version: Some("v1".into()),
        kind: Some("Config".into()),
        clusters: cluster.into_iter().collect(),
        auth_infos: user.into_iter().collect(),
        contexts: vec![NamedContext {
            name: context.to_owned(),
            context: Some(entry),
            ..named.clone()
        }],
        ..Default::default()
    })
}

/// Writes one file per context in `names`, readable only by the user since it holds
/// credentials. Every context must be one [`inspect`] found no problem with, for the same
/// `local`.
pub(crate) fn add(
    dir: &Path,
    new: &Kubeconfig,
    names: &[String],
    existing: &HashSet<String>,
    local: bool,
) -> Result<()> {
    let checked = inspect(new, existing, local);
    for name in names {
        match checked.iter().find(|c| &c.name == name) {
            None => return Err(Error::UnknownContext(name.clone())),
            Some(NewContext { problem: Some(p), .. }) => {
                return Err(Error::InvalidKubeconfig(format!("{name}: {p}")));
            }
            Some(_) => {}
        }
    }
    std::fs::create_dir_all(dir)?;
    for name in names {
        // JSON is YAML too, and kubectl, helm and Kubri read it as a kubeconfig.
        let text = serde_json::to_string_pretty(&single(new, name)?)?;
        let stem = file_stem(name);
        let mut path = dir.join(format!("{stem}.yaml"));
        let mut n = 2;
        while path.exists() {
            path = dir.join(format!("{stem}-{n}.yaml"));
            n += 1;
        }
        write_private(&path, &text)?;
    }
    Ok(())
}

fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(text.as_bytes())
}

/// Removes the added context `name`; false when it was not added in Kubri.
pub(crate) fn remove(dir: &Path, name: &str) -> Result<bool> {
    for file in files_in(dir) {
        let holds = Kubeconfig::read_from(&file).is_ok_and(|k| k.contexts.iter().any(|c| c.name == name));
        if holds {
            std::fs::remove_file(&file)?;
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"
apiVersion: v1
kind: Config
clusters:
- name: default
  cluster: { server: "https://10.0.0.1:6443", certificate-authority-data: Y2E= }
- name: rel
  cluster: { server: "https://10.0.0.2:6443", certificate-authority: ca.crt }
users:
- name: default
  user: { token: secret-token }
- name: eks
  user:
    exec: { apiVersion: client.authentication.k8s.io/v1beta1, command: aws, args: [eks, get-token] }
- name: google
  user:
    auth-provider: { name: gcp, config: { cmd-path: /usr/bin/gcloud, cmd-args: config config-helper } }
- name: from-files
  user: { tokenFile: /var/run/secrets/kubernetes.io/serviceaccount/token }
contexts:
- name: lab
  context: { cluster: default, user: default, namespace: shop }
- name: prod
  context: { cluster: default, user: eks }
- name: relative
  context: { cluster: rel, user: default }
- name: lost
  context: { cluster: nowhere, user: default }
- name: gcp
  context: { cluster: default, user: google }
- name: files
  context: { cluster: default, user: from-files }
"#;

    #[test]
    fn says_what_can_be_added() {
        let new = parse(FILE).unwrap();
        let existing: HashSet<String> = ["prod".to_owned()].into();
        let found = inspect(&new, &existing, true);
        let problem = |name: &str| found.iter().find(|c| c.name == name).unwrap().problem.clone();
        assert_eq!(problem("lab"), None);
        assert!(problem("prod").unwrap().contains("already there"));
        assert!(problem("relative").unwrap().contains("`ca.crt`"));
        assert!(problem("lost").unwrap().contains("`nowhere`"));
        assert_eq!(problem("files"), None);
        assert_eq!(problem("gcp"), None);
        let gcp = found.iter().find(|c| c.name == "gcp").unwrap();
        assert_eq!(gcp.exec.as_deref(), Some("/usr/bin/gcloud"));
        let lab = &found[0];
        assert_eq!(lab.server.as_deref(), Some("https://10.0.0.1:6443"));
        assert_eq!(lab.namespace.as_deref(), Some("shop"));
        assert_eq!(found[1].exec.as_deref(), Some("aws"));
    }

    #[test]
    fn a_server_runs_no_commands_and_reads_no_files_for_added_contexts() {
        let new = parse(FILE).unwrap();
        let found = inspect(&new, &HashSet::new(), false);
        let problem = |name: &str| found.iter().find(|c| c.name == name).unwrap().problem.clone();
        assert_eq!(problem("lab"), None);
        let prod = problem("prod").unwrap();
        assert!(prod.contains("running `aws`"), "{prod}");
        assert!(prod.contains("KUBRI_LOCAL_TERMINAL=true"), "{prod}");
        assert!(problem("gcp").unwrap().contains("running `/usr/bin/gcloud`"));
        let files = problem("files").unwrap();
        assert!(
            files.contains("`/var/run/secrets/kubernetes.io/serviceaccount/token`"),
            "{files}"
        );
        assert!(files.contains("`token`"), "{files}");
        assert!(
            problem("relative")
                .unwrap()
                .contains("`certificate-authority-data`")
        );

        let dir = std::env::temp_dir().join(format!("kubri-added-server-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for name in ["prod", "gcp", "files"] {
            assert!(add(&dir, &new, &[name.into()], &HashSet::new(), false).is_err());
        }
        assert!(files_in(&dir).is_empty());
        add(&dir, &new, &["lab".into()], &HashSet::new(), false).unwrap();
        assert_eq!(files_in(&dir).len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn adds_and_removes_one_file_per_context() {
        let dir = std::env::temp_dir().join(format!("kubri-added-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let new = parse(FILE).unwrap();
        let none = HashSet::new();
        add(&dir, &new, &["lab".into(), "prod".into()], &none, true).unwrap();
        assert!(add(&dir, &new, &["relative".into()], &none, true).is_err());
        let files = files_in(&dir);
        assert_eq!(files.len(), 2);

        let lab = Kubeconfig::read_from(&files[0]).unwrap();
        assert_eq!(lab.contexts[0].name, "lab");
        let entry = lab.contexts[0].context.as_ref().unwrap();
        assert_eq!(entry.cluster, "kubri-lab");
        assert_eq!(entry.user.as_deref(), Some("kubri-lab"));
        assert_eq!(entry.namespace.as_deref(), Some("shop"));
        assert_eq!(lab.clusters[0].name, "kubri-lab");
        use secrecy::ExposeSecret;
        let token = lab.auth_infos[0]
            .auth_info
            .as_ref()
            .unwrap()
            .token
            .as_ref()
            .unwrap();
        assert_eq!(token.expose_secret(), "secret-token");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&files[0]).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        assert!(remove(&dir, "lab").unwrap());
        assert!(!remove(&dir, "lab").unwrap());
        assert_eq!(files_in(&dir).len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn file_names_stay_in_the_folder() {
        assert_eq!(
            file_stem("arn:aws:eks:eu-west-1:1:cluster/prod"),
            "arn_aws_eks_eu-west-1_1_cluster_prod"
        );
        assert_eq!(file_stem("../../etc"), "_.._etc");
    }
}
