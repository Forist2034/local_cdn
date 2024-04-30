use std::{collections::BTreeSet, iter::once, path::Path};

use futures::StreamExt;
use reqwest::Client;
use serde::Serialize;
use tracing_indicatif::span_ext::IndicatifSpanExt;

use npm_registry::{metadata, Registry, Version};

pub async fn fetch_metadata(
    client: &Client,
    name: &str,
) -> Result<metadata::Package, reqwest::Error> {
    client
        .get(format!("https://registry.npmjs.org/{name}"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
}

#[derive(Debug, thiserror::Error)]
pub enum UpdatePkgError {
    #[error("failed to load package info")]
    Load(#[source] npm_registry::LoadError),
    #[error("failed to fetch metadata")]
    FetchMetadata(#[source] reqwest::Error),
}

#[derive(Debug, Serialize)]
pub enum PackageFailure {
    MissingVersion(Vec<Version>),
    UpdateError(log_utils::ReportedErr<UpdatePkgError>),
}
#[derive(Default)]
pub struct FailedPkgInfo(pub Vec<(String, PackageFailure)>);
impl misc_utils::IsEmpty for FailedPkgInfo {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
impl Extend<(String, Result<Vec<Version>, UpdatePkgError>)> for FailedPkgInfo {
    fn extend<T: IntoIterator<Item = (String, Result<Vec<Version>, UpdatePkgError>)>>(
        &mut self,
        iter: T,
    ) {
        self.0
            .extend(iter.into_iter().filter_map(|(name, r)| match r {
                Ok(vs) => {
                    if vs.is_empty() {
                        None
                    } else {
                        Some((name, PackageFailure::MissingVersion(vs)))
                    }
                }
                Err(e) => Some((name, PackageFailure::UpdateError(log_utils::ReportedErr(e)))),
            }))
    }
}
impl Serialize for FailedPkgInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (name, failure) in self.0.iter() {
            map.serialize_entry(name, failure)?;
        }
        map.end()
    }
}

#[tracing::instrument(level="info", skip_all, fields(indicatif.pb_show=tracing::field::Empty))]
async fn fetch_and_update<F, R, P>(
    client: &Client,
    ret: &mut R,
    jobs: usize,
    registry: &mut Registry<P>,
    tasks: Vec<Box<(String, BTreeSet<Version>)>>,
) where
    F: Default + Extend<Version>,
    R: Extend<(String, Result<F, UpdatePkgError>)>,
    P: AsRef<Path>,
{
    log_utils::init_progress("fetching", Some(tasks.len()), &tracing::Span::current());

    let mut s = futures::stream::iter(tasks.into_iter().map(|t| {
        let client = client.clone();
        let span = tracing::Span::current();
        tokio::spawn(async move {
            let r = fetch_metadata(&client, &t.0).await;
            span.pb_inc(1);
            (t, r)
        })
    }))
    .buffer_unordered(jobs);

    while let Some(d) = s.next().await {
        let (task, result) = d.unwrap();
        let _span = tracing::info_span!("update_package", name = task.0).entered();
        match result {
            Ok(meta) => {
                let missing = registry
                    .packages
                    .get_mut(&task.0)
                    .unwrap() // package info is loaded before
                    .0
                    .update_versions(&task.0, task.1, meta);
                ret.extend(once((task.0, Ok(missing))));
            }
            Err(e) => {
                tracing::error!(
                    name = task.0,
                    "failed to fetch metadata: {:?}",
                    log_utils::ReportedErr(&e)
                );
                ret.extend(once((task.0, Err(UpdatePkgError::FetchMetadata(e)))));
            }
        }
    }
}
#[tracing::instrument(level="info", skip_all, fields(jobs, indicatif.pb_show = tracing::field::Empty))]
pub async fn update_pkgs_multi<F, R, P>(
    client: &Client,
    jobs: usize,
    registry: &mut Registry<P>,
    packages: impl IntoIterator<Item = (String, BTreeSet<Version>)>,
) -> R
where
    F: Default + Extend<Version> + Send + 'static,
    R: Default + Extend<(String, Result<F, UpdatePkgError>)>,
    P: AsRef<Path>,
{
    let span = tracing::Span::current();
    span.pb_set_length(0);

    let mut ret = R::default();

    // load all package infos
    span.pb_set_message("loading package info");
    let mut tasks = Vec::new();
    for (name, versions) in packages {
        match registry.load(name.clone()) {
            Ok(n) => {
                if !versions.iter().all(|v| n.versions.contains_key(v)) {
                    tasks.push(Box::new((name, versions)))
                }
            }
            Err(e) => {
                tracing::error!(
                    name,
                    "failed to load package: {:?}",
                    log_utils::ReportedErr(&e)
                );
                ret.extend(once((name, Err(UpdatePkgError::Load(e)))));
            }
        }
    }

    span.pb_set_message("updating metadata");
    fetch_and_update(client, &mut ret, jobs, registry, tasks).await;

    ret
}
