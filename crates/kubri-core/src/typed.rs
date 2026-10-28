//! Lists of built-in types as k8s-openapi structs, for the overview and the XRay tree.

use std::fmt::Debug;

use k8s_openapi::NamespaceResourceScope;
use kube::api::ListParams;
use kube::{Api, Client};
use serde::de::DeserializeOwned;

use crate::{ResourceRef, Result};

/// An API handle for one namespace, or for all of them.
pub(crate) fn scoped<K>(client: &Client, namespace: Option<&str>) -> Api<K>
where
    K: kube::Resource<Scope = NamespaceResourceScope, DynamicType = ()>,
{
    match namespace {
        Some(ns) => Api::namespaced(client.clone(), ns),
        None => Api::all(client.clone()),
    }
}

/// Lists from the API server's cache, which is cheap and at most a moment old.
pub(crate) async fn list<K>(api: Api<K>) -> Result<Vec<K>>
where
    K: kube::Resource + Clone + DeserializeOwned + Debug,
{
    Ok(api.list(&ListParams::default().match_any()).await?.items)
}

/// The namespaces and names of all objects, without reading their contents. Used for
/// Secrets, so their data never leaves the cluster.
pub(crate) async fn names<K>(api: Api<K>) -> Result<Vec<(String, String)>>
where
    K: kube::Resource + Clone + DeserializeOwned + Debug,
{
    let list = api.list_metadata(&ListParams::default().match_any()).await?;
    Ok(list
        .items
        .into_iter()
        .map(|m| {
            (
                m.metadata.namespace.unwrap_or_default(),
                m.metadata.name.unwrap_or_default(),
            )
        })
        .collect())
}

pub(crate) fn resource_ref<K: kube::Resource<DynamicType = ()>>() -> ResourceRef {
    ResourceRef {
        group: K::group(&()).into_owned(),
        version: K::version(&()).into_owned(),
        plural: K::plural(&()).into_owned(),
    }
}
