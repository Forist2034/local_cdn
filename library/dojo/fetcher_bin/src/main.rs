use std::{collections::BTreeSet, process::ExitCode};

use anyhow::{Context, Result};
use clap::Parser;
use log_utils::{LogFormat, LogLevel};

use dojo_fetcher::{FailedVersions, Registry, Version};
use misc_utils::IsEmpty;

#[derive(Debug, clap::Parser)]
struct Cli {
    #[arg(long, default_value_t)]
    log_format: LogFormat,
    #[arg(long, default_value_t)]
    log_level: LogLevel,
    #[arg(long, default_value_t)]
    progress: log_utils::Progress,
    #[arg(long)]
    dest: String,
    #[arg(long)]
    failed: Option<String>,
    sources: Vec<String>,
}

fn run(dest: &str, failed_path: Option<String>, sources: Vec<String>) -> Result<()> {
    let client = reqwest::ClientBuilder::new()
        .https_only(true)
        .build()
        .context("failed to build http client")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to build tokio runtime")?;

    let mut versions = BTreeSet::new();
    for s in sources {
        versions.extend(
            misc_utils::json::read::<Vec<Version>>(&s)
                .with_context(|| format!("failed to read {s}"))?,
        );
    }

    let mut package = Registry::load_or_default(dest).context("failed to read package")?;

    let failed: FailedVersions = runtime.block_on(dojo_fetcher::update_package(
        &client,
        &mut package.package,
        versions,
    ));

    package.save().context("failed to save package info")?;

    if let Some(p) = failed_path {
        if !failed.is_empty() {
            misc_utils::json::write(p, &failed).context("failed to write failed info")?;
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    log_utils::init_tracing(cli.log_level, cli.log_format, cli.progress);
    match run(&cli.dest, cli.failed, cli.sources) {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("failed to fetch package {e:?}");
            ExitCode::FAILURE
        }
    }
}
