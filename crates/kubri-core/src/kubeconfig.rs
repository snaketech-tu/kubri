//! Kubeconfigs read the way kubectl reads them. kube-rs parses the same files, but it accepts
//! and refuses different things than client-go: it stops at a missing file in `KUBECONFIG`,
//! refuses a token user with a leftover client key, sends an empty token instead of reading
//! `tokenFile`, runs an exec plugin next to a client certificate, and takes settings kubectl
//! rejects. This follows client-go's loader, its check before connecting (`ConfirmUsable`)
//! and its transport, with kubectl's messages, so that a context works in Kubri when it works
//! in kubectl and fails when kubectl fails.

use std::path::{Path, PathBuf};

use http::{HeaderName, HeaderValue};
use kube::config::{AuthInfo, Cluster, Kubeconfig, KubeconfigError};
use secrecy::{ExposeSecret, SecretString};

use crate::{Error, Result};

const EXEC_V1: &str = "client.authentication.k8s.io/v1";
const EXEC_V1BETA1: &str = "client.authentication.k8s.io/v1beta1";
const EXEC_V1ALPHA1: &str = "client.authentication.k8s.io/v1alpha1";

/// Loads the files listed in `KUBECONFIG`, merged with the first one winning, or
/// `~/.kube/config`. Listed files that do not exist are skipped, as in kubectl.
pub(crate) fn read() -> Result<Kubeconfig, String> {
    let base = read_own();
    let added = crate::added::files();
    if added.is_empty() {
        return base;
    }
    let added = read_files(&added);
    match (base, added) {
        (Ok(base), Ok(added)) => base.merge(added).map_err(|e| e.to_string()),
        (Ok(base), Err(err)) => {
            tracing::warn!("contexts added in Kubri are left out: {err}");
            Ok(base)
        }
        // Without a kubeconfig of their own, the contexts added in Kubri are all there is.
        (Err(err), Ok(added)) => {
            tracing::warn!("{err}");
            Ok(added)
        }
        (Err(err), Err(_)) => Err(err),
    }
}

/// The value of `KUBECONFIG` for helm and k9s plugins: the user's files, then the contexts
/// added in Kubri.
pub(crate) fn env_value() -> std::ffi::OsString {
    let mut paths: Vec<PathBuf> = match std::env::var_os("KUBECONFIG").filter(|v| !v.is_empty()) {
        Some(value) => std::env::split_paths(&value).collect(),
        None => dirs::home_dir()
            .map(|h| h.join(".kube").join("config"))
            .into_iter()
            .collect(),
    };
    paths.extend(crate::added::files());
    std::env::join_paths(paths).unwrap_or_default()
}

/// The user's own kubeconfig: the files in `KUBECONFIG`, or `~/.kube/config`.
fn read_own() -> Result<Kubeconfig, String> {
    let mut paths: Vec<PathBuf> = Vec::new();
    if let Some(value) = std::env::var_os("KUBECONFIG") {
        for path in std::env::split_paths(&value) {
            if !path.as_os_str().is_empty() && !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    if paths.is_empty() {
        return Kubeconfig::read().map_err(|e| e.to_string());
    }
    read_files(&paths)
}

fn read_files(paths: &[PathBuf]) -> Result<Kubeconfig, String> {
    let mut merged: Option<Kubeconfig> = None;
    for path in paths {
        let next = match Kubeconfig::read_from(path) {
            Ok(next) => next,
            Err(KubeconfigError::ReadConfig(err, _)) if err.kind() == std::io::ErrorKind::NotFound => {
                continue;
            }
            Err(err @ KubeconfigError::ReadConfig(..)) => return Err(err.to_string()),
            Err(err) => return Err(format!("error loading config file \"{}\": {err}", path.display())),
        };
        merged = Some(match merged {
            Some(merged) => merged.merge(next).map_err(|e| e.to_string())?,
            None => next,
        });
    }
    merged.ok_or_else(|| {
        let paths: Vec<_> = paths.iter().map(|p| p.display().to_string()).collect();
        format!("none of the files in KUBECONFIG exist: {}", paths.join(", "))
    })
}

/// Checks the cluster and user of `context` the way kubectl does before it connects, and
/// rewrites the user's credentials so that kube-rs uses what kubectl would use.
pub(crate) fn prepare(kubeconfig: &mut Kubeconfig, context: &str) -> Result<()> {
    let Some(context) = kubeconfig
        .contexts
        .iter()
        .find(|c| c.name == context)
        .and_then(|c| c.context.clone())
    else {
        // kube-rs reports a missing context.
        return Ok(());
    };
    let mut errors = Vec::new();
    let user = context.user.as_deref().and_then(|name| {
        kubeconfig
            .auth_infos
            .iter_mut()
            .find(|u| u.name == name)
            .and_then(|u| u.auth_info.as_mut().map(|info| (name, info)))
    });
    let user = user.map(|(name, user)| {
        clean_user(user);
        check_user(name, user, &mut errors);
        user
    });
    let cluster = kubeconfig
        .clusters
        .iter_mut()
        .find(|c| c.name == context.cluster)
        .and_then(|c| c.cluster.as_mut());
    if let Some(cluster) = cluster {
        clean_cluster(cluster);
        check_cluster(&context.cluster, cluster, &mut errors);
    }
    if !errors.is_empty() {
        return Err(Error::InvalidKubeconfig(errors.join("; ")));
    }
    match user {
        Some(user) => settle_credentials(user),
        None => Ok(()),
    }
}

/// The impersonation headers kube-rs leaves out: `as-uid` and `as-user-extra`. kube-rs sends
/// the user and groups itself.
pub(crate) fn impersonation_headers(user: &AuthInfo) -> Result<Vec<(HeaderName, HeaderValue)>> {
    let mut headers = Vec::new();
    if user.impersonate.is_none() {
        return Ok(headers);
    }
    let invalid = |what: &str| Error::InvalidKubeconfig(format!("invalid {what} to impersonate"));
    if let Some(uid) = &user.impersonate_uid {
        let value = HeaderValue::from_str(uid).map_err(|_| invalid("uid"))?;
        headers.push((HeaderName::from_static("impersonate-uid"), value));
    }
    for (key, values) in user.impersonate_user_extra.iter().flatten() {
        let name = format!("impersonate-extra-{}", header_key_escape(key));
        let name = HeaderName::try_from(name).map_err(|_| invalid("user-extra key"))?;
        for value in values {
            let value = HeaderValue::from_str(value).map_err(|_| invalid("user-extra value"))?;
            headers.push((name.clone(), value));
        }
    }
    Ok(headers)
}

/// Percent-encodes what may not appear in a header name, as client-go's `headerKeyEscape`.
fn header_key_escape(key: &str) -> String {
    let legal = |b: u8| b.is_ascii_alphanumeric() || b"!#$&'*+-.^_`|~".contains(&b);
    key.bytes()
        .map(|b| {
            if legal(b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn empty(value: &Option<String>) -> bool {
    value.as_deref().is_none_or(str::is_empty)
}

fn empty_secret(value: &Option<SecretString>) -> bool {
    value.as_ref().is_none_or(|s| s.expose_secret().is_empty())
}

/// Empty values count as missing in kubectl.
fn clean_cluster(cluster: &mut Cluster) {
    for value in [
        &mut cluster.server,
        &mut cluster.certificate_authority,
        &mut cluster.certificate_authority_data,
        &mut cluster.tls_server_name,
        &mut cluster.proxy_url,
    ] {
        if empty(value) {
            *value = None;
        }
    }
}

fn clean_user(user: &mut AuthInfo) {
    for value in [
        &mut user.token_file,
        &mut user.username,
        &mut user.client_certificate,
        &mut user.client_certificate_data,
        &mut user.client_key,
        &mut user.impersonate,
        &mut user.impersonate_uid,
    ] {
        if empty(value) {
            *value = None;
        }
    }
    for value in [&mut user.token, &mut user.password, &mut user.client_key_data] {
        if empty_secret(value) {
            *value = None;
        }
    }
    if user.impersonate_groups.as_ref().is_some_and(Vec::is_empty) {
        user.impersonate_groups = None;
    }
    if user.impersonate_user_extra.as_ref().is_some_and(|e| e.is_empty()) {
        user.impersonate_user_extra = None;
    }
}

/// client-go's `validateClusterInfo`, plus the TLS check its transport makes.
fn check_cluster(name: &str, cluster: &Cluster, errors: &mut Vec<String>) {
    if cluster.certificate_authority.is_some() && cluster.certificate_authority_data.is_some() {
        errors.push(format!(
            "certificate-authority-data and certificate-authority are both specified for {name}. \
             certificate-authority-data will override."
        ));
    }
    if let Some(path) = &cluster.certificate_authority
        && let Err(err) = std::fs::File::open(path)
    {
        errors.push(format!(
            "unable to read certificate-authority {path} for {name} due to {err}"
        ));
    }
    let has_ca = cluster.certificate_authority.is_some() || cluster.certificate_authority_data.is_some();
    if has_ca && cluster.insecure_skip_tls_verify == Some(true) {
        errors.push("specifying a root certificates file with the insecure flag is not allowed".into());
    }
}

/// client-go's `validateAuthInfo`, plus the checks it makes when it sets up the exec plugin
/// or auth provider.
fn check_user(name: &str, user: &AuthInfo, errors: &mut Vec<String>) {
    let mut methods = Vec::new();
    if user.token.is_some() {
        methods.push("token");
    }
    if user.username.is_some() || user.password.is_some() {
        methods.push("basicAuth");
    }

    if user.client_certificate.is_some() || user.client_certificate_data.is_some() {
        if user.client_certificate.is_some() && user.client_certificate_data.is_some() {
            errors.push(format!(
                "client-cert-data and client-cert are both specified for {name}. \
                 client-cert-data will override."
            ));
        }
        if user.client_key.is_some() && user.client_key_data.is_some() {
            errors.push(format!(
                "client-key-data and client-key are both specified for {name}; \
                 client-key-data will override"
            ));
        }
        if user.client_key.is_none() && user.client_key_data.is_none() {
            errors.push(format!(
                "client-key-data or client-key must be specified for {name} to use the \
                 clientCert authentication method."
            ));
        }
        for (what, path) in [
            ("client-cert", &user.client_certificate),
            ("client-key", &user.client_key),
        ] {
            if let Some(path) = path
                && let Err(err) = std::fs::File::open(path)
            {
                errors.push(format!("unable to read {what} {path} for {name} due to {err}"));
            }
        }
    }

    if let Some(exec) = &user.exec {
        if user.auth_provider.is_some() {
            errors.push(format!(
                "authProvider cannot be provided in combination with an exec plugin for {name}"
            ));
        }
        if empty(&exec.command) {
            errors.push(format!(
                "command must be specified for {name} to use exec authentication plugin"
            ));
        }
        let api_version = exec.api_version.as_deref().unwrap_or_default();
        if api_version.is_empty() {
            errors.push(format!(
                "apiVersion must be specified for {name} to use exec authentication plugin"
            ));
        }
        for env in exec.env.iter().flatten() {
            if env.get("name").is_none_or(String::is_empty) {
                errors.push(format!(
                    "env variable name must be specified for {name} to use exec authentication plugin"
                ));
            }
        }
        // client-go fills in IfAvailable for the beta and alpha versions only.
        if exec.interactive_mode.is_none() && api_version != EXEC_V1BETA1 && api_version != EXEC_V1ALPHA1 {
            errors.push(format!(
                "interactiveMode must be specified for {name} to use exec authentication plugin"
            ));
        }
        if !api_version.is_empty() && api_version != EXEC_V1 && api_version != EXEC_V1BETA1 {
            errors.push(format!("exec plugin: invalid apiVersion \"{api_version}\""));
        }
    }

    if methods.len() > 1 {
        errors.push(format!(
            "more than one authentication method found for {name}; found [{}], only one is allowed",
            methods.join(" ")
        ));
    }

    if (user.impersonate_uid.is_some()
        || user.impersonate_groups.is_some()
        || user.impersonate_user_extra.is_some())
        && user.impersonate.is_none()
    {
        errors.push(format!(
            "requesting uid, groups or user-extra for {name} without impersonating a user"
        ));
    }

    if let Some(provider) = &user.auth_provider {
        match provider.name.as_str() {
            "oidc" => {}
            "gcp" => errors.push(
                "The gcp auth plugin has been removed.\nPlease use the \"gke-gcloud-auth-plugin\" \
                 kubectl/client-go credential plugin instead.\nSee \
                 https://cloud.google.com/blog/products/containers-kubernetes/kubectl-auth-changes-in-gke \
                 for further details"
                    .into(),
            ),
            "azure" => errors.push(
                "The azure auth plugin has been removed.\nPlease use the \
                 https://github.com/Azure/kubelogin kubectl/client-go credential plugin instead.\nSee \
                 https://kubernetes.io/docs/reference/access-authn-authz/authentication/#client-go-credential-plugins \
                 for further details"
                    .into(),
            ),
            other => errors.push(format!("no Auth Provider found for name \"{other}\"")),
        }
    }
}

/// Leaves only the credentials kubectl would use.
fn settle_credentials(user: &mut AuthInfo) -> Result<()> {
    // client-go uses a client certificate only when both halves are set (`HasCertAuth`); a
    // lone key is ignored. A lone certificate was refused in `check_user`.
    let has_cert = user.client_certificate.is_some() || user.client_certificate_data.is_some();
    if !has_cert {
        user.client_key = None;
        user.client_key_data = None;
    }

    // A token from a file has surrounding whitespace trimmed. kube-rs sends it as it is, so
    // such a file is read here, which gives up re-reading it.
    if user.token.is_none()
        && let Some(path) = &user.token_file
    {
        let token = std::fs::read_to_string(path)
            .map_err(|err| Error::InvalidKubeconfig(format!("open {path}: {err}")))?;
        if token.trim() != token {
            user.token = Some(SecretString::from(token.trim().to_owned()));
            user.token_file = None;
        }
    }

    // client-go sends basic auth when a user name is set, with an empty password if none
    // is given; kube-rs needs both.
    if user.username.is_some() && user.password.is_none() {
        user.password = Some(SecretString::from(String::new()));
    }

    // With a token, basic auth or a client certificate, kubectl does not run the exec plugin.
    let static_auth =
        user.token.is_some() || user.token_file.is_some() || user.username.is_some() || has_cert;
    if static_auth {
        user.exec = None;
    }

    if let Some(exec) = &user.exec
        && let Some(command) = &exec.command
        && !command_exists(command)
    {
        let mut message = format!(
            "exec: executable {command} not found\n\nIt looks like you are trying to use a \
             client-go credential plugin that is not installed.\n\nTo learn more about this \
             feature, consult the documentation available at:\n      \
             https://kubernetes.io/docs/reference/access-authn-authz/authentication/#client-go-credential-plugins"
        );
        if let Some(hint) = exec.install_hint.as_deref().filter(|h| !h.is_empty()) {
            message.push_str("\n\n");
            message.push_str(hint);
        }
        return Err(Error::InvalidKubeconfig(message));
    }
    Ok(())
}

/// Whether `command` runs: a path that exists, or a name found on `PATH`.
fn command_exists(command: &str) -> bool {
    let path = Path::new(command);
    if path.components().count() > 1 || path.is_absolute() {
        return path.is_file();
    }
    let extensions: &[&str] = if cfg!(windows) {
        &["", ".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    std::env::var_os("PATH").is_some_and(|dirs| {
        std::env::split_paths(&dirs).any(|dir| {
            extensions
                .iter()
                .any(|ext| dir.join(format!("{command}{ext}")).is_file())
        })
    })
}

#[cfg(test)]
mod tests {
    use kube::Client;

    use super::*;

    // A self-signed certificate and its key, only to have valid PEM data.
    const CERT: &str = "LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCk1JSUJiRENDQVJPZ0F3SUJBZ0lVV1ZybXVCRzlIOWlSVGo1b1d6QkVDNEpSdkg0d0NnWUlLb1pJemowRUF3SXcKRERFS01BZ0dBMVVFQXd3QmVEQWVGdzB5TmpFd01qa3dPVEV5TVRSYUZ3MHlOakV3TXpBd09URXlNVFJhTUF3eApDakFJQmdOVkJBTU1BWGd3V1RBVEJnY3Foa2pPUFFJQkJnZ3Foa2pPUFFNQkJ3TkNBQVJwMFpWV1BzUjEwbW5pCksxSWFwejdydTZjQmkyeVVsdityUGFUOUxqRzhJUU1lc3cxS0ZlK3JJeURNMS8rZ21CQmltZkM0QXBXU0JqOU8Kc3BnOGVyQ2pvMU13VVRBZEJnTlZIUTRFRmdRVXVqT2ZKdVhienU2RmxQejVCUUVSVDNiMFhqMHdId1lEVlIwagpCQmd3Rm9BVXVqT2ZKdVhienU2RmxQejVCUUVSVDNiMFhqMHdEd1lEVlIwVEFRSC9CQVV3QXdFQi96QUtCZ2dxCmhrak9QUVFEQWdOSEFEQkVBaUEzRm5wUStTRjh2cmJ5d04yZGZJeXVLRlpCWDhRQmIwOGs5cDNXZ3owMEd3SWcKV3RzN3F2dHU2NnB4VStoRGk3M28vN0FjU0EwckUyTGdQZTFqNlF3U1NOND0KLS0tLS1FTkQgQ0VSVElGSUNBVEUtLS0tLQo=";
    const KEY: &str = "LS0tLS1CRUdJTiBQUklWQVRFIEtFWS0tLS0tCk1JR0hBZ0VBTUJNR0J5cUdTTTQ5QWdFR0NDcUdTTTQ5QXdFSEJHMHdhd0lCQVFRZ2JJNGl4SitTMVI3dHRuREMKckVLOG5oWEZ4Q0hNa0N3RDk2S0x5WjZxcEV1aFJBTkNBQVJwMFpWV1BzUjEwbW5pSzFJYXB6N3J1NmNCaTJ5VQpsdityUGFUOUxqRzhJUU1lc3cxS0ZlK3JJeURNMS8rZ21CQmltZkM0QXBXU0JqOU9zcGc4ZXJDagotLS0tLUVORCBQUklWQVRFIEtFWS0tLS0tCg==";

    fn kubeconfig(user: &str) -> Kubeconfig {
        kubeconfig_with("{server: 'https://127.0.0.1:6443'}", user)
    }

    fn kubeconfig_with(cluster: &str, user: &str) -> Kubeconfig {
        Kubeconfig::from_yaml(&format!(
            "clusters:
- {{name: c, cluster: {cluster}}}
contexts:
- {{name: x, context: {{cluster: c, user: u}}}}
users:
- {{name: u, user: {user}}}"
        ))
        .unwrap()
    }

    /// The user as it is sent, or kubectl's error.
    fn prepared(user: &str) -> Result<AuthInfo, String> {
        let mut kubeconfig = kubeconfig(user);
        prepare(&mut kubeconfig, "x").map_err(|e| e.to_string())?;
        Ok(kubeconfig.auth_infos[0].auth_info.clone().unwrap())
    }

    fn error(user: &str) -> String {
        prepared(user).unwrap_err()
    }

    fn cluster_error(cluster: &str) -> String {
        let mut kubeconfig = kubeconfig_with(cluster, "{token: t}");
        prepare(&mut kubeconfig, "x").unwrap_err().to_string()
    }

    /// Builds the client the way `Kubri::switch_context` does.
    async fn client(user: &str) -> Result<Client> {
        Ok(Client::try_from(
            crate::client_config(&kubeconfig(user), "x").await?,
        )?)
    }

    fn scratch(name: &str) -> PathBuf {
        crate::k9s::tests::scratch(name)
    }

    #[tokio::test]
    async fn connects_with_a_token_next_to_a_lone_client_key() {
        let user = format!("{{token: secret, client-key-data: {KEY}}}");
        client(&user).await.unwrap();
        let user = prepared(&user).unwrap();
        assert!(user.client_key_data.is_none());
        assert!(user.token.is_some());
    }

    #[tokio::test]
    async fn treats_empty_values_as_missing() {
        client(&format!(
            "{{token: secret, client-certificate-data: '', client-key-data: {KEY}}}"
        ))
        .await
        .unwrap();
        client("{token: secret, client-certificate: '', client-key: /no/such/key}")
            .await
            .unwrap();

        let dir = scratch("kubeconfig-empty-token");
        std::fs::write(dir.join("token"), "from-file").unwrap();
        let user = prepared(&format!(
            "{{token: '', tokenFile: '{}'}}",
            dir.join("token").display()
        ))
        .unwrap();
        assert!(user.token.is_none());
        assert!(user.token_file.is_some());
    }

    #[tokio::test]
    async fn keeps_a_complete_client_certificate() {
        let user = format!("{{client-certificate-data: {CERT}, client-key-data: {KEY}}}");
        client(&user).await.unwrap();
        let user = prepared(&user).unwrap();
        assert!(user.client_certificate_data.is_some());
        assert!(user.client_key_data.is_some());
    }

    #[test]
    fn refuses_what_kubectl_refuses_for_client_certificates() {
        assert_eq!(
            error(&format!("{{client-certificate-data: {CERT}}}")),
            "invalid kubeconfig: client-key-data or client-key must be specified for u to use \
             the clientCert authentication method."
        );
        assert!(
            error(&format!(
                "{{client-certificate: /c, client-certificate-data: {CERT}, client-key-data: {KEY}}}"
            ))
            .contains("client-cert-data and client-cert are both specified for u.")
        );
        assert!(
            error(&format!(
                "{{client-certificate-data: {CERT}, client-key: /k, client-key-data: {KEY}}}"
            ))
            .contains("client-key-data and client-key are both specified for u;")
        );
        assert!(
            error(&format!(
                "{{client-certificate: /no/such.crt, client-key-data: {KEY}}}"
            ))
            .contains("unable to read client-cert /no/such.crt for u due to")
        );
    }

    #[test]
    fn refuses_two_ways_to_log_in() {
        assert_eq!(
            error("{token: t, username: a, password: b}"),
            "invalid kubeconfig: more than one authentication method found for u; found \
             [token basicAuth], only one is allowed"
        );
        // A token next to a client certificate is fine: kubectl sends both.
        let user = prepared(&format!(
            "{{token: t, client-certificate-data: {CERT}, client-key-data: {KEY}}}"
        ))
        .unwrap();
        assert!(user.token.is_some() && user.client_certificate_data.is_some());
    }

    #[test]
    fn sends_basic_auth_with_an_empty_password_like_kubectl() {
        let user = prepared("{username: admin}").unwrap();
        assert_eq!(user.password.unwrap().expose_secret(), "");
    }

    #[test]
    fn trims_a_token_file_like_kubectl() {
        let dir = scratch("kubeconfig-token-file");
        let path = dir.join("token");
        std::fs::write(&path, "abc\n").unwrap();
        let user = prepared(&format!("{{tokenFile: '{}'}}", path.display())).unwrap();
        assert_eq!(user.token.unwrap().expose_secret(), "abc");
        assert!(user.token_file.is_none());

        // Without whitespace the file stays, so that a rotated token is picked up.
        std::fs::write(&path, "abc").unwrap();
        let user = prepared(&format!("{{tokenFile: '{}'}}", path.display())).unwrap();
        assert!(user.token.is_none());
        assert!(user.token_file.is_some());

        assert!(error("{tokenFile: /no/such/token}").contains("open /no/such/token:"));
    }

    #[test]
    fn skips_the_exec_plugin_when_kubectl_does() {
        let exec = "{apiVersion: client.authentication.k8s.io/v1beta1, command: 'false'}";
        let user = prepared(&format!(
            "{{client-certificate-data: {CERT}, client-key-data: {KEY}, exec: {exec}}}"
        ))
        .unwrap();
        assert!(user.exec.is_none());
        assert!(
            prepared(&format!("{{token: t, exec: {exec}}}"))
                .unwrap()
                .exec
                .is_none()
        );
        assert!(prepared(&format!("{{exec: {exec}}}")).unwrap().exec.is_some());
    }

    #[test]
    fn checks_exec_plugins_like_kubectl() {
        let plugin = |extra: &str| format!("{{exec: {{command: 'true'{extra}}}}}");
        assert!(prepared(&plugin(", apiVersion: client.authentication.k8s.io/v1beta1")).is_ok());
        assert!(
            prepared(&plugin(
                ", apiVersion: client.authentication.k8s.io/v1, interactiveMode: Never"
            ))
            .is_ok()
        );
        assert!(
            error(&plugin(", apiVersion: client.authentication.k8s.io/v1"))
                .contains("interactiveMode must be specified for u to use exec authentication plugin")
        );
        let missing = error(&plugin(""));
        assert!(
            missing.contains("apiVersion must be specified for u"),
            "{missing}"
        );
        // Defaulted like v1beta1, so only the version itself is refused.
        assert_eq!(
            error(&plugin(", apiVersion: client.authentication.k8s.io/v1alpha1")),
            "invalid kubeconfig: exec plugin: invalid apiVersion \
             \"client.authentication.k8s.io/v1alpha1\""
        );
        assert!(
            error(&plugin(
                ", apiVersion: client.authentication.k8s.io/v1beta1, env: [{name: '', value: a}]"
            ))
            .contains("env variable name must be specified for u")
        );
        assert!(
            error(
                "{auth-provider: {name: oidc}, exec: {command: 'true', apiVersion: \
                 client.authentication.k8s.io/v1beta1}}"
            )
            .contains("authProvider cannot be provided in combination with an exec plugin for u")
        );
    }

    #[test]
    fn shows_the_install_hint_of_a_missing_exec_plugin() {
        let err = error(
            "{exec: {apiVersion: client.authentication.k8s.io/v1beta1, command: kubri-no-such-plugin, \
             installHint: 'brew install it'}}",
        );
        assert!(
            err.contains("exec: executable kubri-no-such-plugin not found"),
            "{err}"
        );
        assert!(err.ends_with("\n\nbrew install it"), "{err}");
        assert!(command_exists("sh"));
        assert!(!command_exists("/no/such/plugin"));
    }

    #[test]
    fn refuses_auth_providers_kubectl_removed() {
        assert!(error("{auth-provider: {name: gcp}}").contains("The gcp auth plugin has been removed."));
        assert!(error("{auth-provider: {name: azure}}").contains("The azure auth plugin has been removed."));
        assert!(error("{auth-provider: {name: foo}}").contains("no Auth Provider found for name \"foo\""));
        assert!(prepared("{auth-provider: {name: oidc, config: {id-token: t}}}").is_ok());
    }

    #[test]
    fn impersonates_like_kubectl() {
        assert!(
            error("{token: t, as-groups: [admins]}")
                .contains("requesting uid, groups or user-extra for u without impersonating a user")
        );
        let user = prepared(
            "{token: t, as: jane, as-uid: '42', as-user-extra: {scopes: [a, b], 'acme.com/team': [x]}}",
        )
        .unwrap();
        let headers: Vec<_> = impersonation_headers(&user)
            .unwrap()
            .into_iter()
            .map(|(k, v)| format!("{k}: {}", v.to_str().unwrap()))
            .collect();
        assert!(headers.contains(&"impersonate-uid: 42".to_owned()), "{headers:?}");
        assert!(headers.contains(&"impersonate-extra-scopes: a".to_owned()));
        assert!(headers.contains(&"impersonate-extra-scopes: b".to_owned()));
        assert!(
            headers.contains(&"impersonate-extra-acme.com%2fteam: x".to_owned()),
            "{headers:?}"
        );
        assert!(
            impersonation_headers(&prepared("{token: t}").unwrap())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn checks_the_cluster_like_kubectl() {
        let dir = scratch("kubeconfig-ca");
        let ca = dir.join("ca.crt");
        std::fs::write(&ca, "").unwrap();
        let ca = ca.display();
        assert!(
            cluster_error(&format!(
                "{{server: 'https://k', certificate-authority: '{ca}', certificate-authority-data: {CERT}}}"
            ))
            .contains("certificate-authority-data and certificate-authority are both specified for c.")
        );
        assert!(
            cluster_error("{server: 'https://k', certificate-authority: /no/such/ca.crt}")
                .contains("unable to read certificate-authority /no/such/ca.crt for c due to")
        );
        assert!(
            cluster_error(&format!(
                "{{server: 'https://k', insecure-skip-tls-verify: true, certificate-authority-data: {CERT}}}"
            ))
            .contains("specifying a root certificates file with the insecure flag is not allowed")
        );
        let mut ok = kubeconfig_with(
            "{server: 'https://k', insecure-skip-tls-verify: true, certificate-authority-data: ''}",
            "{token: t}",
        );
        prepare(&mut ok, "x").unwrap();
        assert!(
            ok.clusters[0]
                .cluster
                .as_ref()
                .unwrap()
                .certificate_authority_data
                .is_none()
        );
    }

    #[test]
    fn skips_missing_files_in_kubeconfig_like_kubectl() {
        let dir = scratch("kubeconfig-files");
        let first = dir.join("first.yaml");
        let second = dir.join("second.yaml");
        std::fs::write(&first, "contexts:\n- {name: a, context: {cluster: c}}\n").unwrap();
        std::fs::write(
            &second,
            "contexts:\n- {name: a, context: {cluster: d}}\n- {name: b, context: {cluster: c}}\n",
        )
        .unwrap();
        let missing = dir.join("missing.yaml");

        let merged = read_files(&[missing.clone(), first.clone(), missing.clone(), second]).unwrap();
        let contexts: Vec<_> = merged
            .contexts
            .iter()
            .map(|c| (c.name.as_str(), c.context.as_ref().unwrap().cluster.as_str()))
            .collect();
        assert_eq!(contexts, [("a", "c"), ("b", "c")]);

        let err = read_files(std::slice::from_ref(&missing)).unwrap_err();
        assert!(err.starts_with("none of the files in KUBECONFIG exist:"), "{err}");

        let broken = dir.join("broken.yaml");
        std::fs::write(&broken, "users: [ {").unwrap();
        let err = read_files(&[first, broken.clone()]).unwrap_err();
        assert!(
            err.starts_with(&format!("error loading config file \"{}\":", broken.display())),
            "{err}"
        );
    }
}
