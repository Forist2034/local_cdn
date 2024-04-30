use std::{
    collections::{hash_map, BTreeMap, HashMap},
    fs, io,
    iter::once,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use ssri::Integrity;

pub use node_semver::Version;

pub mod metadata {
    use std::collections::BTreeMap;

    use serde::Deserialize;
    use ssri::Integrity;

    #[derive(Debug, Deserialize)]
    pub struct Dist {
        pub tarball: String,
        pub integrity: Integrity,
    }
    #[derive(Deserialize)]
    pub struct VersionInfo {
        pub dist: Dist,
    }

    #[derive(Deserialize)]
    pub struct Package {
        pub name: String,
        pub versions: BTreeMap<node_semver::Version, VersionInfo>,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDist {
    pub drv_name: String,
    pub url: String,
    pub hash: Integrity,
}

pub type LoadError = misc_utils::json::ReadError;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub drv_name: String,
    pub versions: BTreeMap<Version, VersionDist>, // deterministic order
}
impl Package {
    pub fn new(name: String) -> Self {
        Self {
            drv_name: misc_utils::to_drv_name(&name),
            versions: BTreeMap::new(),
        }
    }
    pub fn load_or_default(
        path: impl AsRef<Path>,
        name: impl FnOnce() -> String,
    ) -> Result<Self, LoadError> {
        let path = path.as_ref();
        if path.exists() {
            misc_utils::json::read(path)
        } else {
            Ok(Self::new(name()))
        }
    }
    pub fn add_npm_dist(&mut self, name: &str, v: Version, dist: metadata::Dist) {
        self.versions.insert(
            v.clone(),
            VersionDist {
                drv_name: misc_utils::to_drv_name(&format!("{name}-{v}")),
                url: dist.tarball,
                hash: dist.integrity,
            },
        );
    }
    pub fn update_versions<F: Default + Extend<Version>>(
        &mut self,
        name: &str,
        vs: impl IntoIterator<Item = Version>,
        mut npm: metadata::Package,
    ) -> F {
        let mut missing = F::default();
        for v in vs.into_iter() {
            match npm.versions.remove(&v) {
                Some(d) => self.add_npm_dist(name, v, d.dist),
                None => {
                    tracing::warn!(version = v.to_string(), "missing version");
                    missing.extend(once(v));
                }
            }
        }
        missing
    }
}

#[derive(Debug)]
pub struct Registry<P> {
    pub root: P,
    pub packages: HashMap<String, (Package, PathBuf)>,
}

#[derive(Debug, thiserror::Error)]
#[error("failed to save {name}")]
pub struct SaveError {
    name: String,
    source: io::Error,
}

pub const REGISTRY_PATH: &str = "source/npm";
impl<P> Registry<P> {
    pub fn new(root: P) -> Self {
        Self {
            root,
            packages: HashMap::new(),
        }
    }
}
impl<P: AsRef<Path>> Registry<P> {
    pub fn load(&mut self, name: String) -> Result<&Package, LoadError> {
        Ok(self.load_mut(name)?)
    }
    pub fn load_mut(&mut self, name: String) -> Result<&mut Package, LoadError> {
        match self.packages.entry(name) {
            hash_map::Entry::Occupied(o) => Ok(&mut o.into_mut().0),
            hash_map::Entry::Vacant(v) => {
                let path = self.root.as_ref().join(format!("{}.json", v.key()));
                let pkg = Package::load_or_default(&path, || v.key().clone())?;
                Ok(&mut v.insert((pkg, path)).0)
            }
        }
    }
    pub fn save(&self) -> Result<(), SaveError> {
        for (name, (pkg, path)) in self.packages.iter() {
            if let Some(p) = path.parent() {
                if !p.exists() {
                    fs::create_dir_all(p).map_err(|e| SaveError {
                        name: name.to_owned(),
                        source: e,
                    })?;
                }
            }
            misc_utils::json::write(path, pkg).map_err(|e| SaveError {
                name: name.to_owned(),
                source: e,
            })?;
        }
        Ok(())
    }
}
