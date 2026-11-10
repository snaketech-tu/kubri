//! The calls both apps offer the UI, in one table: each is a variant of [`Request`] with the
//! value it answers, and [`Kubri::call`] runs it. The web server takes them at `POST /api/call`
//! and the desktop app as its `call` command, so a new call is written once, here. Calls that
//! stream (tables, logs, terminals) keep their own sockets and channels.
//!
//! The UI's types come from the same table: ts-rs writes `Request` and `Responses`, the
//! answer of each call by its name.

use serde::Deserialize;
use serde_json::Value;

use crate::{ClusterInfo, ContextList, KubeconfigFile, Kubri, NewContext, Result};

/// Each line is `name { arguments } -> answer => |kubri| body;`, where the body may use the
/// arguments by name. Names are camelCase on the wire: `AddContexts` is `addContexts`.
macro_rules! calls {
    ($(
        $(#[doc = $doc:literal])*
        $name:ident { $($field:ident : $ty:ty),* $(,)? } -> $reply:ty => |$kubri:ident| $body:expr;
    )*) => {
        /// A call from the UI, named by `call`, with its arguments next to it.
        #[derive(Debug, Clone, Deserialize, ts_rs::TS)]
        #[serde(tag = "call", rename_all = "camelCase", rename_all_fields = "camelCase")]
        #[ts(export)]
        pub enum Request {
            $(
                $(#[doc = $doc])*
                $name { $($field: $ty),* },
            )*
        }

        /// What each call answers, by its name. Only its type is used, by the UI.
        #[allow(dead_code, non_snake_case)]
        #[derive(ts_rs::TS)]
        #[ts(export, rename_all = "camelCase")]
        pub struct Responses {
            $(pub $name: $reply,)*
        }

        impl Kubri {
            /// Runs `request` and answers with its value as JSON.
            pub async fn call(&self, request: Request) -> Result<Value> {
                match request {
                    $(
                        Request::$name { $($field),* } => {
                            let $kubri = self;
                            let reply: $reply = $body;
                            Ok(serde_json::to_value(reply)?)
                        }
                    )*
                }
            }
        }
    };
}

calls! {
    /// Every context, and the one Kubri is connected to.
    Contexts {} -> ContextList => |kubri| kubri.contexts().await?;
    /// Reads the kubeconfig files again.
    ReloadContexts {} -> ContextList => |kubri| kubri.reload().await?;
    /// The kubeconfig files Kubri reads.
    KubeconfigFiles {} -> Vec<KubeconfigFile> => |kubri| kubri.kubeconfig_files();
    /// The contexts of a pasted kubeconfig, before adding them.
    InspectKubeconfig { text: String } -> Vec<NewContext> => |kubri| kubri.inspect_kubeconfig(&text).await?;
    /// Adds the contexts `names` of a pasted kubeconfig.
    AddContexts { text: String, names: Vec<String> } -> ContextList =>
        |kubri| kubri.add_contexts(&text, &names).await?;
    /// Removes a context added in Kubri.
    RemoveContext { name: String } -> ContextList => |kubri| kubri.remove_context(&name).await?;
    /// Connects to another context.
    SwitchContext { name: String } -> ClusterInfo => |kubri| kubri.switch_context(&name).await?;
    /// Stops a switch that is still connecting.
    CancelSwitch {} -> () => |kubri| kubri.cancel_switch().await;
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn reads_the_call_and_its_arguments_by_name() {
        let request: Request =
            serde_json::from_value(json!({ "call": "addContexts", "text": "x", "names": ["a"] })).unwrap();
        assert!(matches!(request, Request::AddContexts { text, names } if text == "x" && names == ["a"]));
        let request: Request = serde_json::from_value(json!({ "call": "contexts" })).unwrap();
        assert!(matches!(request, Request::Contexts {}));
        assert!(serde_json::from_value::<Request>(json!({ "call": "switchContext" })).is_err());
        assert!(serde_json::from_value::<Request>(json!({ "call": "nothing" })).is_err());
    }

    #[tokio::test]
    async fn runs_a_call_and_answers_with_its_value() {
        let kubeconfig: kube::config::Kubeconfig = serde_saphyr::from_str(
            "contexts:\n- name: one\n  context: {cluster: c, user: u}\nclusters:\n- name: c\n  cluster: {server: 'https://127.0.0.1:1'}\nusers:\n- name: u\n  user: {}\n",
        )
        .unwrap();
        let kubri = Kubri::from_kubeconfig(kubeconfig);
        let reply = kubri.call(Request::Contexts {}).await.unwrap();
        assert_eq!(reply["contexts"][0]["name"], "one");
        assert_eq!(reply["cluster"], Value::Null);
        assert_eq!(kubri.call(Request::CancelSwitch {}).await.unwrap(), Value::Null);
    }
}
