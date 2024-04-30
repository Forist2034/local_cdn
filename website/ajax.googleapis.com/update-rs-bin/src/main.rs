use std::{path::Path, process::ExitCode};

use anyhow::{Context, Result};
use clap::Parser;

#[derive(Debug, clap::Parser)]
struct Cli {
    #[arg(long, default_value_t)]
    log_level: log_utils::LogLevel,
    #[arg(long, default_value_t)]
    log_format: log_utils::LogFormat,
    #[arg(long, default_value_t)]
    progress: log_utils::Progress,
    #[arg(long, default_value_t = 1)]
    jobs: usize,
    #[arg(long)]
    root: String,
    #[arg(long)]
    failed: String,
    data: String,
}

fn run(jobs: usize, root: impl AsRef<Path>, failed: String, data: String) -> Result<()> {
    let client = reqwest::ClientBuilder::new()
        .https_only(true)
        .build()
        .context("failed to build client")?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to build tokio runtime")?;

    let root = root.as_ref();

    let mut npm_reg = npm_registry::Registry::new(root.join(npm_registry::REGISTRY_PATH));
    let mut three = three_js::Registry::load_or_default(root.join(three_js::REGISTRY_PATH))
        .context("failed to load three.js store")?;
    // let mut jquery_ui =
    //     jquery_fetcher::PackageStore::load_or_default(root.join(jquery_fetcher::UI_PKG_PATH))
    //         .context("failed to load jquery ui store")?;
    // let mut jquery_mobile =
    //     jquery_fetcher::PackageStore::load_or_default(root.join(jquery_fetcher::MOBILE_PKG_PATH))
    //         .context("failed to load jquery mobile store")?;
    let mut dojo = dojo_fetcher::Registry::load_or_default(root.join(dojo_fetcher::REGISTRY_PATH))
        .context("failed to load dojo store")?;

    runtime.block_on(google_ajax::update_google_ajax(
        &client,
        jobs,
        data,
        failed,
        &mut npm_reg,
        &mut three,
        // &mut jquery_ui,
        // &mut jquery_mobile,
        &mut dojo,
    ));

    npm_reg.save().context("failed to save npm registry")?;
    three.save().context("failed to save three.js data")?;
    // jquery_ui.save().context("failed to save jquery-ui data")?;
    // jquery_mobile
    // .save()
    // .context("failed to save jquery-mobile data")?;
    dojo.save().context("failed to save dojo data")?;
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    log_utils::init_tracing(cli.log_level, cli.log_format, cli.progress);

    match run(cli.jobs, cli.root, cli.failed, cli.data) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("failed to update google ajax libraries: {e:?}");
            ExitCode::FAILURE
        }
    }
}
