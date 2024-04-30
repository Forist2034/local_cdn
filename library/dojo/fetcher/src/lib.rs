use std::{collections::BTreeMap, io, iter::once, path::Path};

use reqwest::Client;
use serde::{Deserialize, Serialize};

pub use node_semver::Version;
use tracing_indicatif::span_ext::IndicatifSpanExt;

/// min supported version
pub const MIN_VERSION: Version = Version {
    major: 1,
    minor: 0,
    patch: 0,
    build: Vec::new(),
    pre_release: Vec::new(),
};

fn serialize_md5<S: serde::Serializer>(value: &[u8; 16], serializer: S) -> Result<S::Ok, S::Error> {
    let mut buf = [0; 32];
    const_hex::encode_to_slice(value, &mut buf).unwrap();
    serializer.serialize_str(std::str::from_utf8(&buf).unwrap())
}
#[derive(Debug, Serialize, Deserialize)]
pub struct VersionInfo {
    pub url: String,
    #[serde(
        serialize_with = "serialize_md5",
        deserialize_with = "const_hex::serde::deserialize"
    )]
    pub md5: [u8; 16],
}

pub type LoadError = misc_utils::json::ReadError;

#[derive(Debug, Serialize, Deserialize)]
pub struct Package {
    pub versions: BTreeMap<Version, VersionInfo>,
}
impl Package {
    pub fn load_or_default(path: impl AsRef<Path>) -> Result<Self, LoadError> {
        let p = path.as_ref();
        if p.exists() {
            misc_utils::json::read(path)
        } else {
            Ok(Self {
                versions: BTreeMap::new(),
            })
        }
    }
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), io::Error> {
        misc_utils::json::write(path, self)
    }
}

pub struct Registry<P> {
    path: P,
    pub package: Package,
}
pub const REGISTRY_PATH: &str = "library/dojo/package.json";
impl<P: AsRef<Path>> Registry<P> {
    pub fn load_or_default(path: P) -> Result<Self, LoadError> {
        Ok(Self {
            package: Package::load_or_default(path.as_ref())?,
            path,
        })
    }
    pub fn save(&self) -> Result<(), io::Error> {
        self.package.save(self.path.as_ref())
    }
}

fn parse_md5(file: &str) -> Option<[u8; 16]> {
    const_hex::FromHex::from_hex(file.trim().split_whitespace().next()?).ok()
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("unsupported version")]
    Unsupported,
    #[error("failed to get md5 file")]
    Http(reqwest::Error),
    #[error("invalid md5 file: {0:?}")]
    InvalidData(String),
}
pub async fn fetch_version(client: &Client, version: &Version) -> Result<VersionInfo, UpdateError> {
    if version < &MIN_VERSION {
        return Err(UpdateError::Unsupported);
    }
    let url =
        format!("https://download.dojotoolkit.org/release-{version}/dojo-release-{version}.tar.gz");
    let md5_file = client
        .get(&format!("{url}.md5"))
        .send()
        .await
        .map_err(UpdateError::Http)?
        .error_for_status()
        .map_err(UpdateError::Http)?
        .text()
        .await
        .map_err(UpdateError::Http)?;
    Ok(VersionInfo {
        url,
        md5: parse_md5(&md5_file).ok_or_else(|| UpdateError::InvalidData(md5_file))?,
    })
}

#[derive(Debug, Default)]
pub struct FailedVersions(pub Vec<(Version, log_utils::ReportedErr<UpdateError>)>);
impl misc_utils::IsEmpty for FailedVersions {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
impl Extend<(Version, UpdateError)> for FailedVersions {
    fn extend<T: IntoIterator<Item = (Version, UpdateError)>>(&mut self, iter: T) {
        self.0.extend(
            iter.into_iter()
                .map(|(v, e)| (v, log_utils::ReportedErr(e))),
        )
    }
}
impl Serialize for FailedVersions {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (v, e) in self.0.iter() {
            map.serialize_entry(v, e)?;
        }
        map.end()
    }
}

#[tracing::instrument(skip_all, fields(indicatif.pb_show = tracing::field::Empty))]
pub async fn updated_versions<VS, F>(
    client: &Client,
    package: &Package,
    versions: impl IntoIterator<Item = Version>,
) -> (VS, F)
where
    VS: Default + Extend<(Version, VersionInfo)>,
    F: Default + Extend<(Version, UpdateError)>,
{
    let span = tracing::Span::current();
    let versions = versions.into_iter();
    log_utils::init_progress("updating dojo tookit", versions.size_hint().1, &span);

    let mut updated = VS::default();
    let mut failed = F::default();
    for v in versions {
        if package.versions.contains_key(&v) {
            span.pb_inc(1);
            continue;
        }
        span.pb_set_message(&format!("updating dojo {v}"));
        match fetch_version(client, &v).await {
            Ok(vi) => {
                tracing::info!(version = v.to_string(), "fetched version");
                span.pb_inc(1);
                updated.extend(once((v, vi)));
            }
            Err(e) => {
                tracing::error!(
                    version = v.to_string(),
                    "failed to fetch: {}",
                    log_utils::ReportedErr(&e)
                );
                span.pb_inc(1);
                failed.extend(once((v, e)));
            }
        }
    }
    (updated, failed)
}

pub async fn update_package<F: Default + Extend<(Version, UpdateError)>>(
    client: &Client,
    package: &mut Package,
    versions: impl IntoIterator<Item = Version>,
) -> F {
    let (updated, failed) = updated_versions::<Vec<_>, _>(client, package, versions).await;
    package.versions.extend(updated);
    failed
}

#[cfg(test)]
mod test {
    use crate::parse_md5;

    #[test]
    fn md5_data() {
        assert_eq!(
            parse_md5("d2ce2672a5ee8bff5b675175a364be6b  dojo-release-1.17.3.tar.gz").unwrap(),
            hex_literal::hex!("d2ce2672a5ee8bff5b675175a364be6b")
        )
    }
}
