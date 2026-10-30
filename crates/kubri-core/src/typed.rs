//! Lists of built-in types as k8s-openapi structs, for the overview and the XRay tree.

use std::fmt::Debug;

use k8s_openapi::NamespaceResourceScope;
use kube::api::ListParams;
use kube::core::Request;
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

/// Lists from the API server's cache, which is cheap and at most a moment old. Each object's
/// `managedFields` are dropped as it is read: nothing here uses them, and they can make up half
/// of an object, so a list of 10,000 pods holds much less.
pub(crate) async fn list<K>(api: Api<K>) -> Result<Vec<K>>
where
    K: kube::Resource + Clone + DeserializeOwned + Debug,
{
    let request = Request::new(api.resource_url())
        .list(&ListParams::default().match_any())
        .map_err(kube::Error::BuildRequest)?;
    let client: Client = api.into();
    let text = client.request_text(request).await?;
    let list: Lean<K> = serde_json::from_str(&text)?;
    Ok(list.items.0)
}

#[derive(serde::Deserialize)]
#[serde(bound = "K: kube::Resource + DeserializeOwned")]
struct Lean<K> {
    #[serde(default)]
    items: Items<K>,
}

struct Items<K>(Vec<K>);

impl<K> Default for Items<K> {
    fn default() -> Self {
        Items(Vec::new())
    }
}

impl<'de, K: kube::Resource + DeserializeOwned> serde::Deserialize<'de> for Items<K> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct Visit<K>(std::marker::PhantomData<K>);
        impl<'de, K: kube::Resource + DeserializeOwned> serde::de::Visitor<'de> for Visit<K> {
            type Value = Items<K>;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a list of objects")
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Items<K>, A::Error> {
                let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0));
                while let Some(mut item) = seq.next_element::<K>()? {
                    item.meta_mut().managed_fields = None;
                    items.push(item);
                }
                Ok(Items(items))
            }

            fn visit_unit<E>(self) -> std::result::Result<Items<K>, E> {
                Ok(Items::default())
            }
        }
        deserializer.deserialize_any(Visit(std::marker::PhantomData))
    }
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
