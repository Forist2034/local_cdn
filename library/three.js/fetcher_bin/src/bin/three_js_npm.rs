use std::{collections::BTreeSet, fs, process::ExitCode};

use anyhow::{Context, Result};
use clap::Parser;
use reqwest::blocking::Client;

use three_js::ThreeVersion;

fn run(
    npm_root: String,
    ver_map_path: String,
    failed: Option<String>,
    sources: Vec<String>,
) -> Result<()> {
    let ver: BTreeSet<ThreeVersion> = {
        let mut ret = BTreeSet::new();
        for s in sources {
            ret.extend(
                serde_json::from_slice::<Vec<ThreeVersion>>(
                    &fs::read(&s).with_context(|| format!("failed to read input {s}"))?,
                )
                .with_context(|| format!("failed to decode input {s}"))?
                .into_iter(),
            );
        }
        ret
    };

    let meta = Client::builder()
        .https_only(true)
        .build()
        .context("failed to build client")?
        .get("https://registry.npmjs.org/three")
        .send()
        .context("failed to send http request")?
        .error_for_status()
        .context("status error")?
        .json()
        .context("failed to get http response")?;
    let mut npm_reg = npm_registry::Registry::new(npm_root);
    let mut ver_map =
        three_js::Registry::load_or_default(ver_map_path).context("failed to load version map")?;

    let fail = three_js::update_registries::<Vec<_>, _, _>(&mut npm_reg, &mut ver_map, ver, meta)
        .context("failed to update npm registry")?;
    npm_reg.save().context("failed to save npm registry")?;
    ver_map.save().context("failed to save version map")?;

    if let Some(p) = failed {
        if !fail.is_empty() {
            fs::write(p, serde_json::to_vec_pretty(&fail).unwrap())
                .context("failed to write failed")?;
        }
    }
    Ok(())
}

#[derive(Debug, clap::Parser)]
struct Cli {
    #[arg(long)]
    failed: Option<String>,
    #[arg(long)]
    version_map: String,
    #[arg(long)]
    npm_root: String,
    source: Vec<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    log_utils::init_tracing(
        log_utils::LogLevel::default(),
        log_utils::LogFormat::default(),
        log_utils::Progress::Off,
    );

    if let Err(e) = run(cli.npm_root, cli.version_map, cli.failed, cli.source) {
        tracing::error!("{e:?}");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
