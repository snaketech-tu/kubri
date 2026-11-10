//! Server-side dry runs: what an object would look like after a change, beside how it is now,
//! for the UI to show as a diff before anything changes.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ContainerImage;

/// A change to preview, as the actions make it.
#[derive(Debug, Clone, Deserialize, ts_rs::TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export)]
pub enum Change {
    /// The whole object, as a YAML edit saves it.
    Replace {
        #[ts(type = "unknown")]
        value: Value,
    },
    /// A JSON merge patch, as labels and annotations are changed.
    Patch {
        #[ts(type = "unknown")]
        patch: Value,
    },
    Scale {
        replicas: u32,
    },
    /// `kubectl rollout restart`.
    Restart,
    Images {
        images: Vec<ContainerImage>,
    },
    /// One container's memory limit in a workload's pod template.
    Memory {
        container: String,
        memory: String,
    },
    /// Requests and limits of containers in a workload's pod template.
    Resources {
        containers: Vec<crate::rightsizing::ContainerResources>,
    },
}

/// An object now and after the change, without what changes on every write.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Preview {
    pub before: Value,
    pub after: Value,
}

/// Leaves out `managedFields`, `resourceVersion`, `generation` and `status`, which differ
/// after every write, like `kubectl diff` does.
pub(crate) fn comparable(mut object: Value) -> Value {
    if let Some(map) = object.as_object_mut() {
        map.remove("status");
        if let Some(meta) = map.get_mut("metadata").and_then(Value::as_object_mut) {
            for key in ["managedFields", "resourceVersion", "generation"] {
                meta.remove(key);
            }
        }
    }
    object
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_out_server_fields() {
        let object = serde_json::json!({
            "metadata": { "name": "web", "resourceVersion": "7", "generation": 3, "managedFields": [{}], "labels": { "a": "b" } },
            "spec": { "replicas": 2 },
            "status": { "replicas": 2 }
        });
        assert_eq!(
            comparable(object),
            serde_json::json!({ "metadata": { "name": "web", "labels": { "a": "b" } }, "spec": { "replicas": 2 } })
        );
    }

    #[test]
    fn reads_changes() {
        let change: Change =
            serde_json::from_value(serde_json::json!({ "type": "scale", "replicas": 3 })).unwrap();
        assert!(matches!(change, Change::Scale { replicas: 3 }));
        let change: Change = serde_json::from_value(serde_json::json!({ "type": "restart" })).unwrap();
        assert!(matches!(change, Change::Restart));
    }
}
