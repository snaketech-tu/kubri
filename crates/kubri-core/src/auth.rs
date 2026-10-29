//! Client credentials as kubectl reads them. kube-rs refuses a user with a client key but no
//! client certificate ("failed to load client certificate: no base64 data or file"), while
//! kubectl (client-go's `HasCertAuth`) only uses a client certificate when both halves are
//! there and otherwise ignores them. Kubeconfigs from some providers carry a leftover
//! `client-key-data` next to a `token`, so kubectl connects and kube-rs does not.

use kube::config::AuthInfo;

/// Drops a client key without a certificate, which kubectl ignores. Empty values count as
/// missing, as in kubectl. A certificate without a key stays, since kubectl rejects it too.
pub(crate) fn ignore_lone_client_key(user: &mut AuthInfo) {
    let empty = |value: &Option<String>| value.as_deref().is_none_or(str::is_empty);
    let has_cert = !empty(&user.client_certificate_data) || !empty(&user.client_certificate);
    if has_cert {
        return;
    }
    if user.client_key_data.is_some() || user.client_key.is_some() {
        tracing::debug!("ignoring a client key without a client certificate, like kubectl");
    }
    user.client_certificate_data = None;
    user.client_certificate = None;
    user.client_key_data = None;
    user.client_key = None;
}

#[cfg(test)]
mod tests {
    use kube::Client;
    use kube::config::Kubeconfig;

    use super::*;

    // A self-signed certificate and its key, only to have valid PEM data.
    const CERT: &str = "LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCk1JSUJiRENDQVJPZ0F3SUJBZ0lVV1ZybXVCRzlIOWlSVGo1b1d6QkVDNEpSdkg0d0NnWUlLb1pJemowRUF3SXcKRERFS01BZ0dBMVVFQXd3QmVEQWVGdzB5TmpFd01qa3dPVEV5TVRSYUZ3MHlOakV3TXpBd09URXlNVFJhTUF3eApDakFJQmdOVkJBTU1BWGd3V1RBVEJnY3Foa2pPUFFJQkJnZ3Foa2pPUFFNQkJ3TkNBQVJwMFpWV1BzUjEwbW5pCksxSWFwejdydTZjQmkyeVVsdityUGFUOUxqRzhJUU1lc3cxS0ZlK3JJeURNMS8rZ21CQmltZkM0QXBXU0JqOU8Kc3BnOGVyQ2pvMU13VVRBZEJnTlZIUTRFRmdRVXVqT2ZKdVhienU2RmxQejVCUUVSVDNiMFhqMHdId1lEVlIwagpCQmd3Rm9BVXVqT2ZKdVhienU2RmxQejVCUUVSVDNiMFhqMHdEd1lEVlIwVEFRSC9CQVV3QXdFQi96QUtCZ2dxCmhrak9QUVFEQWdOSEFEQkVBaUEzRm5wUStTRjh2cmJ5d04yZGZJeXVLRlpCWDhRQmIwOGs5cDNXZ3owMEd3SWcKV3RzN3F2dHU2NnB4VStoRGk3M28vN0FjU0EwckUyTGdQZTFqNlF3U1NOND0KLS0tLS1FTkQgQ0VSVElGSUNBVEUtLS0tLQo=";
    const KEY: &str = "LS0tLS1CRUdJTiBQUklWQVRFIEtFWS0tLS0tCk1JR0hBZ0VBTUJNR0J5cUdTTTQ5QWdFR0NDcUdTTTQ5QXdFSEJHMHdhd0lCQVFRZ2JJNGl4SitTMVI3dHRuREMKckVLOG5oWEZ4Q0hNa0N3RDk2S0x5WjZxcEV1aFJBTkNBQVJwMFpWV1BzUjEwbW5pSzFJYXB6N3J1NmNCaTJ5VQpsdityUGFUOUxqRzhJUU1lc3cxS0ZlK3JJeURNMS8rZ21CQmltZkM0QXBXU0JqOU9zcGc4ZXJDagotLS0tLUVORCBQUklWQVRFIEtFWS0tLS0tCg==";

    fn kubeconfig(user: &str) -> Kubeconfig {
        Kubeconfig::from_yaml(&format!(
            "clusters:
- {{name: c, cluster: {{server: 'https://127.0.0.1:6443'}}}}
contexts:
- {{name: x, context: {{cluster: c, user: u}}}}
users:
- {{name: u, user: {user}}}"
        ))
        .unwrap()
    }

    /// Builds the client the way `Kubri::switch_context` does.
    async fn client(user: &str) -> kube::Result<Client> {
        Client::try_from(crate::client_config(&kubeconfig(user), "x").await.unwrap())
    }

    #[tokio::test]
    async fn connects_with_a_token_next_to_a_lone_client_key() {
        let user = format!("{{token: secret, client-key-data: {KEY}}}");
        client(&user).await.unwrap();

        let mut config = kubeconfig(&user).auth_infos[0].auth_info.clone().unwrap();
        ignore_lone_client_key(&mut config);
        assert!(config.client_key_data.is_none());
        assert!(config.token.is_some());
    }

    #[tokio::test]
    async fn treats_an_empty_certificate_as_missing() {
        client(&format!(
            "{{token: secret, client-certificate-data: '', client-key-data: {KEY}}}"
        ))
        .await
        .unwrap();
        client("{token: secret, client-certificate: '', client-key: /no/such/key}")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn keeps_a_complete_client_certificate() {
        let user = format!("{{client-certificate-data: {CERT}, client-key-data: {KEY}}}");
        client(&user).await.unwrap();

        let mut config = kubeconfig(&user).auth_infos[0].auth_info.clone().unwrap();
        ignore_lone_client_key(&mut config);
        assert!(config.client_certificate_data.is_some());
        assert!(config.client_key_data.is_some());
    }

    #[tokio::test]
    async fn still_rejects_a_certificate_without_a_key() {
        let Err(err) = client(&format!("{{client-certificate-data: {CERT}}}")).await else {
            panic!("a certificate without a key was accepted");
        };
        assert!(err.to_string().contains("client key"), "{err}");
    }
}
