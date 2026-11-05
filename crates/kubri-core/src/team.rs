//! A Kubri server shared by a team. Each person who signs in gets a [`Kubri`] of their own,
//! made with [`Kubri::for_person`], that reaches the clusters with that person's own
//! Kubernetes permissions.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use kube::Config;
use kube::config::{AuthInfo, Kubeconfig, NamedAuthInfo};
use serde::Serialize;

use crate::{Error, Kubri, Result};

/// Someone signed in to a Kubri shared by a team.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Person {
    /// The Kubernetes user name the person acts as.
    pub name: String,
    pub groups: Vec<String>,
}

/// How a person's requests reach the clusters.
#[derive(Debug, Clone)]
pub enum ActAs {
    /// With the server's own credentials, asking the cluster to treat every request as the
    /// person's (`Impersonate-User` and `Impersonate-Group`). The server's account needs the
    /// `impersonate` verb for users and groups.
    Impersonate(Person),
    /// With the person's own token from signing in, for clusters that accept tokens of the
    /// same issuer. The server keeps the newest token in `dir/token`; Kubri reads it again
    /// every minute.
    Token { person: Person, dir: PathBuf },
}

impl ActAs {
    pub fn person(&self) -> &Person {
        match self {
            ActAs::Impersonate(person) | ActAs::Token { person, .. } => person,
        }
    }

    /// The file the person's token is kept in, when Kubri passes it on.
    pub fn token_file(&self) -> Option<PathBuf> {
        match self {
            ActAs::Impersonate(_) => None,
            ActAs::Token { dir, .. } => Some(dir.join("token")),
        }
    }
}

/// Why something is not offered on a Kubri shared by a team.
pub(crate) const NO_ADDED_CONTEXTS: &str =
    "on a Kubri shared by a team, the contexts come from the server's kubeconfig";
pub(crate) const NO_PLUGINS: &str =
    "k9s plugins run with the server's own credentials, so a Kubri shared by a team does not run them";
pub(crate) const NO_LOCAL_TERMINAL: &str = "A Kubri shared by a team has no local terminal: it would run on the server, with the server's own credentials.";

/// Makes `config` act as the person instead of as the server.
pub(crate) fn apply(config: &mut Config, act_as: &ActAs) {
    match act_as {
        ActAs::Impersonate(person) => {
            let user = &mut config.auth_info;
            user.impersonate = Some(person.name.clone());
            user.impersonate_groups = (!person.groups.is_empty()).then(|| person.groups.clone());
            user.impersonate_uid = None;
            user.impersonate_user_extra = None;
        }
        ActAs::Token { .. } => {
            // Nothing of the server's own user is left: no certificate, token or command.
            config.auth_info = AuthInfo {
                token_file: act_as.token_file().map(|p| p.to_string_lossy().into_owned()),
                ..AuthInfo::default()
            };
        }
    }
}

/// How `helm` runs for the person: the `KUBECONFIG` it reads and the flags it gets.
pub(crate) struct HelmAccess {
    pub kubeconfig: std::ffi::OsString,
    pub args: Vec<String>,
}

pub(crate) fn helm_access(act_as: Option<&ActAs>, kubeconfig: &Kubeconfig) -> Result<HelmAccess> {
    match act_as {
        None => Ok(HelmAccess {
            kubeconfig: crate::kubeconfig::env_value(),
            args: Vec::new(),
        }),
        Some(ActAs::Impersonate(person)) => {
            // One argument each, so a name cannot pass for a flag.
            let mut args = vec![format!("--kube-as-user={}", person.name)];
            args.extend(person.groups.iter().map(|g| format!("--kube-as-group={g}")));
            Ok(HelmAccess {
                kubeconfig: crate::kubeconfig::env_value(),
                args,
            })
        }
        Some(act_as @ ActAs::Token { dir, .. }) => {
            // The server's kubeconfig with every user swapped for the person's token, so helm
            // cannot fall back on the server's own certificate.
            let path = dir.join("kubeconfig");
            write_private(&path, &person_kubeconfig(kubeconfig, act_as)?)?;
            Ok(HelmAccess {
                kubeconfig: path.into_os_string(),
                args: Vec::new(),
            })
        }
    }
}

fn person_kubeconfig(kubeconfig: &Kubeconfig, act_as: &ActAs) -> Result<String> {
    let mut kubeconfig = kubeconfig.clone();
    let token_file = act_as.token_file().map(|p| p.to_string_lossy().into_owned());
    kubeconfig.auth_infos = kubeconfig
        .auth_infos
        .iter()
        .map(|named| NamedAuthInfo {
            name: named.name.clone(),
            auth_info: Some(AuthInfo {
                token_file: token_file.clone(),
                ..AuthInfo::default()
            }),
            ..NamedAuthInfo::default()
        })
        .collect();
    serde_json::to_string(&kubeconfig).map_err(|e| Error::Unsupported(e.to_string()))
}

/// Writes `text` to `path`, readable only by this user.
pub fn write_private(path: &Path, text: &str) -> Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(text.as_bytes())?;
    Ok(())
}

impl Kubri {
    /// A Kubri for one person on a server shared by a team. It shares this Kubri's
    /// kubeconfig and settings, is read-only when this one is, and has no local terminal,
    /// plugins, added contexts or change recording: those would run with the server's own
    /// credentials, or show what others may see.
    pub fn for_person(&self, act_as: ActAs) -> Kubri {
        let mut kubri = Kubri::sharing(self.kubeconfig.clone(), self.settings.clone());
        kubri.act_as = Some(act_as);
        if self.read_only.load(std::sync::atomic::Ordering::Relaxed) {
            kubri.set_read_only();
        }
        kubri
    }

    /// The person this Kubri acts for, on a server shared by a team.
    pub fn person(&self) -> Option<&Person> {
        self.act_as.as_ref().map(ActAs::person)
    }

    /// Fails when this Kubri acts for a person on a server shared by a team.
    pub(crate) fn ensure_not_team(&self, why: &str) -> Result<()> {
        match self.act_as {
            Some(_) => Err(Error::Unsupported(why.to_owned())),
            None => Ok(()),
        }
    }

    pub(crate) async fn helm_access(&self) -> Result<HelmAccess> {
        let kubeconfig = match &*self.kubeconfig.read().await {
            Ok(kubeconfig) => kubeconfig.clone(),
            Err(err) => return Err(Error::NoKubeconfig(err.clone())),
        };
        helm_access(self.act_as.as_ref(), &kubeconfig)
    }
}

/// A shared handle on the settings, for [`Kubri::for_person`].
pub(crate) type SharedSettings = Arc<std::sync::RwLock<crate::SettingsFile>>;

#[cfg(test)]
mod tests {
    use super::*;

    fn alice() -> Person {
        Person {
            name: "alice@example.com".into(),
            groups: vec!["devs".into(), "oncall".into()],
        }
    }

    async fn config() -> Config {
        let kubeconfig = Kubeconfig::from_yaml(
            r#"
clusters: [{name: c, cluster: {server: "https://192.0.2.1:6443"}}]
users: [{name: u, user: {token: server-token, as: someone, as-uid: "7", as-user-extra: {a: [b]}}}]
contexts: [{name: x, context: {cluster: c, user: u}}]
"#,
        )
        .unwrap();
        crate::client_config(&kubeconfig, "x", None).await.unwrap()
    }

    #[tokio::test]
    async fn impersonation_replaces_whoever_the_kubeconfig_acts_as() {
        let mut config = config().await;
        apply(&mut config, &ActAs::Impersonate(alice()));
        let user = &config.auth_info;
        assert_eq!(user.impersonate.as_deref(), Some("alice@example.com"));
        assert_eq!(
            user.impersonate_groups,
            Some(vec!["devs".into(), "oncall".into()])
        );
        assert_eq!(user.impersonate_uid, None);
        assert_eq!(user.impersonate_user_extra, None);
        // The server's own token still authenticates the requests.
        assert!(user.token.is_some());

        let mut config = self::config().await;
        let nobody = Person {
            name: "bob".into(),
            groups: vec![],
        };
        apply(&mut config, &ActAs::Impersonate(nobody));
        assert_eq!(config.auth_info.impersonate_groups, None);
    }

    #[tokio::test]
    async fn a_passed_on_token_leaves_nothing_of_the_server() {
        let mut config = config().await;
        let act_as = ActAs::Token {
            person: alice(),
            dir: PathBuf::from("/run/kubri/x"),
        };
        apply(&mut config, &act_as);
        assert!(config.auth_info.token.is_none());
        assert_eq!(config.auth_info.impersonate, None);
        assert_eq!(config.auth_info.token_file.as_deref(), Some("/run/kubri/x/token"));
    }

    #[tokio::test]
    async fn the_identity_reaches_every_client() {
        let kubeconfig = Kubeconfig::from_yaml(
            r#"
clusters: [{name: c, cluster: {server: "https://192.0.2.1:6443"}}]
users: [{name: u, user: {token: t}}]
contexts: [{name: x, context: {cluster: c, user: u}}]
"#,
        )
        .unwrap();
        let act_as = ActAs::Impersonate(alice());
        let config = crate::client_config(&kubeconfig, "x", Some(&act_as))
            .await
            .unwrap();
        assert_eq!(config.auth_info.impersonate.as_deref(), Some("alice@example.com"));
    }

    #[test]
    fn helm_acts_as_the_person() {
        let kubeconfig = Kubeconfig::from_yaml(
            r#"
clusters: [{name: c, cluster: {server: "https://192.0.2.1:6443"}}]
users: [{name: u, user: {client-key-data: c2VjcmV0, client-certificate-data: Y2VydA==}}]
contexts: [{name: x, context: {cluster: c, user: u}}]
"#,
        )
        .unwrap();
        assert!(helm_access(None, &kubeconfig).unwrap().args.is_empty());
        let access = helm_access(Some(&ActAs::Impersonate(alice())), &kubeconfig).unwrap();
        assert_eq!(
            access.args,
            [
                "--kube-as-user=alice@example.com",
                "--kube-as-group=devs",
                "--kube-as-group=oncall"
            ]
        );

        let dir = std::env::temp_dir().join(format!("kubri-team-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let act_as = ActAs::Token {
            person: alice(),
            dir: dir.clone(),
        };
        let access = helm_access(Some(&act_as), &kubeconfig).unwrap();
        assert_eq!(access.kubeconfig, dir.join("kubeconfig").into_os_string());
        let written = std::fs::read_to_string(dir.join("kubeconfig")).unwrap();
        assert!(!written.contains("c2VjcmV0"), "{written}");
        let written = Kubeconfig::from_yaml(&written).unwrap();
        let user = written.auth_infos[0].auth_info.as_ref().unwrap();
        assert_eq!(
            user.token_file,
            Some(dir.join("token").to_string_lossy().into_owned())
        );
        assert!(user.client_key_data.is_none());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join("kubeconfig"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn a_person_cannot_add_contexts_or_run_plugins() {
        let kubri = Kubri::from_kubeconfig(Kubeconfig::default());
        let person = kubri.for_person(ActAs::Impersonate(alice()));
        assert_eq!(person.person(), Some(&alice()));
        let refused = person.add_contexts("apiVersion: v1", &[]).await.unwrap_err();
        assert!(refused.to_string().contains("shared by a team"), "{refused}");
        let refused = person.add_kubeconfig_files(&["/etc/x".into()]).await.unwrap_err();
        assert!(refused.to_string().contains("shared by a team"), "{refused}");
        let info = person.local_terminal_info().await;
        assert!(!info.enabled);
        assert!(info.reason.unwrap_or_default().contains("shared by a team"));
    }

    #[tokio::test]
    async fn people_share_the_settings_but_not_the_connection() {
        let kubri = Kubri::from_kubeconfig(Kubeconfig::default());
        kubri.set_read_only();
        let alice = kubri.for_person(ActAs::Impersonate(alice()));
        let mut searches = alice.settings().searches;
        searches.push(crate::SavedSearch {
            name: "pods".into(),
            query: "kind:pod".into(),
        });
        alice.set_saved_searches(searches).unwrap();
        assert_eq!(kubri.settings().searches.len(), 1);
        assert!(alice.guard().await.read_only || alice.cluster().await.is_none());
        assert!(alice.read_only.load(std::sync::atomic::Ordering::Relaxed));
    }
}
