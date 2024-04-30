use std::{
    collections::{BTreeSet, HashMap},
    fs,
    future::Future,
    path::Path,
};

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, PartialOrd, Ord, Deserialize)]
struct VersionInfo<V> {
    version: V,
}

async fn update<P, F, Fut>(
    root: &Path,
    failed_root: &Path,
    name: &str,
    update: impl FnOnce(P) -> Fut,
) where
    P: serde::de::DeserializeOwned,
    F: misc_utils::IsEmpty + Serialize,
    Fut: Future<Output = Result<F>>,
{
    let file_name = format!("{name}.json");
    let ret: Result<()> = async {
        let packages = serde_json::from_slice::<P>(
            &fs::read(root.join(&file_name)).context("failed to read package file")?,
        )
        .context("failed to parse package file")?;
        let failed = update(packages).await?;
        if !failed.is_empty() {
            fs::write(
                failed_root.join(&file_name),
                serde_json::to_vec_pretty(&failed).unwrap(),
            )
            .context("failed to write failed info")?;
        }
        Ok(())
    }
    .await;
    match ret {
        Ok(()) => tracing::info!("updated {name}"),
        Err(e) => tracing::error!("failed to update {name} {e:?}"),
    }
}

pub async fn update_google_ajax<P1, P2, /*P3, P4,*/ P5>(
    client: &Client,
    jobs: usize,
    root: impl AsRef<Path>,
    failed_root: impl AsRef<Path>,
    npm_reg: &mut npm_registry::Registry<P1>,
    three: &mut three_js::Registry<P2>,
    // jquery_ui: &mut jquery_fetcher::PackageStore<P3>,
    // jquery_mobile: &mut jquery_fetcher::PackageStore<P4>,
    dojo: &mut dojo_fetcher::Registry<P5>,
) where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
    // P3: AsRef<Path>,
    // P4: AsRef<Path>,
    P5: AsRef<Path>,
{
    let root = root.as_ref();
    let failed_root = failed_root.as_ref();

    update(
        root,
        failed_root,
        "npm",
        |packages: HashMap<String, BTreeSet<VersionInfo<npm_registry::Version>>>| async {
            Ok(
                npm_fetcher::update_pkgs_multi::<_, npm_fetcher::FailedPkgInfo, _>(
                    client,
                    jobs,
                    npm_reg,
                    packages
                        .into_iter()
                        .map(|(p, v)| (p, v.into_iter().map(|v| v.version).collect())),
                )
                .await,
            )
        },
    )
    .await;

    update(
        root,
        failed_root,
        "three",
        |versions: BTreeSet<VersionInfo<three_js::ThreeVersion>>| async {
            three_js::update_registries::<Vec<_>, _, _>(
                npm_reg,
                three,
                versions.into_iter().map(|v| v.version),
                npm_fetcher::fetch_metadata(client, three_js::NPM_PACKAGE_NAME)
                    .await
                    .context("failed to fetch metadata")?,
            )
            .context("failed to update metadata")
        },
    )
    .await;

    // update(
    //     root,
    //     failed_root,
    //     "jquery-ui",
    //     |versions: BTreeSet<jquery_fetcher::Version>| async {
    //         Ok(
    //             jquery_fetcher::update_package::<jquery_fetcher::FailedVersions>(
    //                 client,
    //                 jquery_fetcher::PackageName::JQueryUI,
    //                 jobs,
    //                 versions,
    //                 &mut jquery_ui.package,
    //             )
    //             .await,
    //         )
    //     },
    // )
    // .await;

    // update(
    //     root,
    //     failed_root,
    //     "jquery-mobile",
    //     |versions: BTreeSet<jquery_fetcher::Version>| async {
    //         Ok(
    //             jquery_fetcher::update_package::<jquery_fetcher::FailedVersions>(
    //                 client,
    //                 jquery_fetcher::PackageName::JQueryMobile,
    //                 jobs,
    //                 versions,
    //                 &mut jquery_mobile.package,
    //             )
    //             .await,
    //         )
    //     },
    // )
    // .await;

    update(
        root,
        failed_root,
        "dojo",
        |versions: BTreeSet<dojo_fetcher::Version>| async {
            Ok(
                dojo_fetcher::update_package::<dojo_fetcher::FailedVersions>(
                    client,
                    &mut dojo.package,
                    versions,
                )
                .await,
            )
        },
    )
    .await;
}
