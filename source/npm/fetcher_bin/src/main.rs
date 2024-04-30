use std::{
    collections::{hash_map, BTreeSet, HashMap},
    fs,
    process::ExitCode,
};

use anyhow::{Context, Result};
use clap::Parser;
use npm_fetcher::FailedPkgInfo;
use npm_registry::{Registry, Version};
use serde::{de, Deserialize};

use log_utils::{init_tracing, LogFormat, LogLevel};

struct VersionList(Vec<(String, Vec<Version>)>);
impl<'de> Deserialize<'de> for VersionList {
    fn deserialize<D>(deserializer: D) -> std::prelude::v1::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = VersionList;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a map of version")
            }
            fn visit_map<A>(self, mut map: A) -> std::prelude::v1::Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                let mut ret = match map.size_hint() {
                    Some(s) => Vec::with_capacity(s),
                    None => Vec::new(),
                };
                while let Some(p) = map.next_entry()? {
                    ret.push(p)
                }
                Ok(VersionList(ret))
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

fn run(
    plan_par: usize,
    spec_path: Vec<String>,
    root: String,
    failed_path: Option<String>,
) -> Result<()> {
    let mut packages: HashMap<String, BTreeSet<Version>> = HashMap::new();
    for i in spec_path {
        let sp: VersionList = fs::read(&i)
            .context("failed to read file")
            .and_then(|f| serde_json::from_slice(&f).context("failed to decode json"))
            .with_context(|| format!("failed to load spec {i}"))?;
        for (name, versions) in sp.0 {
            match packages.entry(name) {
                hash_map::Entry::Occupied(mut o) => o.get_mut().extend(versions),
                hash_map::Entry::Vacant(v) => {
                    v.insert(versions.into_iter().collect());
                }
            }
        }
    }

    let client = reqwest::Client::builder()
        .https_only(true)
        .build()
        .context("failed to build http client")?;
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to build tokio runtime")?;
    let mut registry = Registry::new(root);

    let failed = rt.block_on(npm_fetcher::update_pkgs_multi::<_, FailedPkgInfo, _>(
        &client,
        plan_par,
        &mut registry,
        packages.into_iter(),
    ));
    registry.save().context("failed to save package info")?;
    if let Some(p) = failed_path {
        if !failed.0.is_empty() {
            fs::write(p, serde_json::to_vec_pretty(&failed).unwrap())
                .context("failed to write failed info")?;
        }
    }
    Ok(())
}

#[derive(clap::Parser)]
struct Cli {
    #[arg(long)]
    jobs: Option<usize>,
    #[arg(long, default_value_t)]
    log_format: LogFormat,
    #[arg(long, default_value_t)]
    log_level: LogLevel,
    #[arg(long, default_value_t)]
    progress: log_utils::Progress,
    #[arg(long)]
    root: String,
    #[arg(long)]
    failed: Option<String>,
    spec_paths: Vec<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    init_tracing(cli.log_level, cli.log_format, cli.progress);

    if let Err(e) = run(cli.jobs.unwrap_or(1), cli.spec_paths, cli.root, cli.failed) {
        tracing::error!("failed to get packages: {e:?}");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
