use std::{
    fmt::Display,
    io,
    process::{Output, Stdio},
};

use reqwest::Client;
use serde::Deserialize;
use ssri::Integrity;
use tokio::io::AsyncWriteExt;
use tracing::Instrument;

#[derive(Debug, thiserror::Error)]
pub enum FetchFileError {
    #[error("failed to read http request")]
    Http(#[source] reqwest::Error),
    #[error("failed to write data")]
    Io(#[source] io::Error),
}
#[tracing::instrument(level = tracing::Level::INFO, skip(client, writer))]
pub async fn fetch_file<W: tokio::io::AsyncWrite + Unpin>(
    client: &Client,
    url: &str,
    mut writer: W,
) -> Result<(), FetchFileError> {
    #[cfg(feature = "progress")]
    use tracing_indicatif::span_ext::IndicatifSpanExt;

    let mut r = client
        .get(url)
        .send()
        .await
        .map_err(FetchFileError::Http)?
        .error_for_status()
        .map_err(FetchFileError::Http)?;

    if let Some(l) = r.content_length() {
        tracing::trace!(content_length = l, "received response");
    }

    let span = tracing::info_span!("reading_body", indicatif.pb_show = tracing::field::Empty);
    #[cfg(feature = "progress")]
    {
        if let Some(l) = r.content_length() {
            span.pb_set_style(
                &indicatif::ProgressStyle::with_template(
                    "{wide_bar} {bytes}/{total_bytes} {percent}%",
                )
                .unwrap(),
            );
            span.pb_set_length(l);
        } else {
            span.pb_set_style(
                &indicatif::ProgressStyle::with_template("{spinner} {bytes}").unwrap(),
            );
            span.pb_set_length(0);
        }
    }

    async {
        #[cfg(feature = "progress")]
        let span = tracing::Span::current();

        while let Some(v) = r.chunk().await.map_err(FetchFileError::Http)? {
            writer
                .write_all(v.as_ref())
                .await
                .map_err(FetchFileError::Io)?;

            tracing::trace!(len = v.len(), "received {} bytes data", v.len());

            #[cfg(feature = "progress")]
            span.pb_inc(v.len() as u64);
        }
        Ok(())
    }
    .instrument(span)
    .await?;

    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct StoredFile {
    pub hash: Integrity,
    #[serde(rename = "storePath")]
    pub store_path: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AddStoreError {
    Spawn(#[source] io::Error),
    Wait(#[source] io::Error),
    Store(Output),
    InvalidOutput {
        output: Output,
        #[source]
        source: serde_json::Error,
    },
}
impl Display for AddStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn show_output(out: &Output, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match out.status.code() {
                Some(c) => write!(f, "nix command exits with {c}")?,
                None => f.write_str("nix command is terminated")?,
            }
            match std::str::from_utf8(&out.stdout) {
                Ok(v) => write!(f, " stdout: {v}")?,
                Err(_) => write!(f, " stdout (binary): {:x?}", out.stdout)?,
            }
            match std::str::from_utf8(&out.stderr) {
                Ok(v) => write!(f, " stderr: {v}"),
                Err(_) => write!(f, " stderr(binary): {:x?}", out.stderr),
            }
        }
        match &self {
            Self::Spawn(_) => f.write_str("failed to spawn process"),
            Self::Wait(_) => f.write_str("failed to get process output"),
            Self::Store(o) => show_output(o, f),
            Self::InvalidOutput { output, .. } => {
                f.write_str("decode json error")?;
                show_output(output, f)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HashAlgo {
    Md5,
    Sha1,
    #[default]
    Sha256,
    Sha512,
}

#[derive(Debug)]
pub struct AddOptions {
    pub executable: bool,
    pub hash_algo: HashAlgo,
    pub expected_hash: Option<Integrity>,
}
impl Default for AddOptions {
    fn default() -> Self {
        Self {
            executable: false,
            hash_algo: HashAlgo::default(),
            expected_hash: None,
        }
    }
}

#[tracing::instrument(level = tracing::Level::INFO)]
pub async fn add_to_store(
    options: AddOptions,
    name: &str,
    path: &str,
) -> Result<StoredFile, AddStoreError> {
    let mut proc = tokio::process::Command::new("nix");
    proc.args([
        "store",
        "prefetch-file",
        "--json",
        "--name",
        name,
        "--hash-type",
        match options.hash_algo {
            HashAlgo::Md5 => "md5",
            HashAlgo::Sha1 => "sha1",
            HashAlgo::Sha256 => "sha256",
            HashAlgo::Sha512 => "sha512",
        },
    ]);
    if options.executable {
        proc.arg("--executable");
    }
    if let Some(h) = options.expected_hash {
        proc.args(["--expected-hash", &h.to_string()]);
    }
    proc.arg(format!("file://{}", path));
    let result = proc
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(AddStoreError::Spawn)?
        .wait_with_output()
        .await
        .map_err(AddStoreError::Wait)?;
    if !result.status.success() {
        Err(AddStoreError::Store(result))
    } else {
        Ok(
            serde_json::from_slice(&result.stdout).map_err(|e| AddStoreError::InvalidOutput {
                output: result,
                source: e,
            })?,
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PrefetchFileError {
    #[error("failed to create tmp file")]
    CreateTmp(#[source] io::Error),
    #[error("failed to open tmp file")]
    OpenTmp(#[source] io::Error),
    #[error("failed to fetch dist")]
    FetchFile(#[source] FetchFileError),
    #[error("path contains non utf8 character")]
    InvalidPath,
    #[error("failed to add unpacked dist to store")]
    AddStore(#[source] AddStoreError),
}

#[tracing::instrument(level = tracing::Level::INFO, skip(client), fields(indicatif.pb_show = tracing::field::Empty))]
pub async fn prefetch_file(
    client: &Client,
    options: AddOptions,
    name: &str,
    url: &str,
) -> Result<StoredFile, PrefetchFileError> {
    #[cfg(feature = "progress")]
    use tracing_indicatif::span_ext::IndicatifSpanExt;

    let span = tracing::Span::current();
    #[cfg(feature = "progress")]
    span.pb_set_style(
        &indicatif::ProgressStyle::with_template(&format!(
            "{{spinner}} fetching {name} from {url} {{wide_msg}}"
        ))
        .unwrap(),
    );

    let file = tempfile::Builder::new()
        .tempfile()
        .map_err(PrefetchFileError::CreateTmp)?;
    tracing::debug!("temp file created");

    #[cfg(feature = "progress")]
    span.pb_set_message("fetching dist package");

    fetch_file(
        client,
        url,
        tokio::fs::File::from_std(file.reopen().map_err(PrefetchFileError::OpenTmp)?),
    )
    .await
    .map_err(PrefetchFileError::FetchFile)?;

    #[cfg(feature = "progress")]
    span.pb_set_message("adding dist package to store");

    add_to_store(
        options,
        name,
        file.path().to_str().ok_or(PrefetchFileError::InvalidPath)?,
    )
    .await
    .map_err(PrefetchFileError::AddStore)
}
