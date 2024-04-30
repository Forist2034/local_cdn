use std::{collections::BTreeMap, fmt::Display, io, iter::once, path::Path, str::FromStr};

use npm_registry::metadata;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ThreeVersion(pub u16);
impl Display for ThreeVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "r{}", self.0)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid version {0}")]
pub struct ParseVersionErr(String);
impl FromStr for ThreeVersion {
    type Err = ParseVersionErr;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s
            .strip_prefix('r')
            .ok_or_else(|| ParseVersionErr(s.to_string()))?
            .parse()
        {
            Ok(v) => Ok(Self(v)),
            Err(_) => Err(ParseVersionErr(s.to_string())),
        }
    }
}
impl Serialize for ThreeVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
impl<'de> Deserialize<'de> for ThreeVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de;
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = ThreeVersion;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("three.js version")
            }
            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                v.parse().map_err(E::custom)
            }
        }
        deserializer.deserialize_str(Visitor)
    }
}

pub type VersionMap = BTreeMap<ThreeVersion, npm_registry::Version>;

pub struct PackageInfo<V, VM, F> {
    pub npm_info: V,
    pub version_map: VM,
    /// missing npm versions
    pub missing: F,
}

pub fn npm_versions<V, VM, F>(
    ver: impl IntoIterator<Item = ThreeVersion>,
    meta: &metadata::Package,
) -> PackageInfo<V, VM, F>
where
    V: Default + Extend<npm_registry::Version>,
    VM: Default + Extend<(ThreeVersion, npm_registry::Version)>,
    F: Default + Extend<ThreeVersion>,
{
    let mut versions = V::default();
    let mut ver_map = VM::default();
    let mut missing = F::default();

    'v: for i in ver {
        for (v, _) in meta.versions.iter().rev() {
            if v.minor as u16 == i.0 {
                versions.extend(once(v.clone()));
                ver_map.extend(once((i, v.clone())));
                continue 'v;
            }
        }
        tracing::warn!(version = i.0, "missing version");
        missing.extend(once(i));
    }
    PackageInfo {
        npm_info: versions,
        version_map: ver_map,
        missing,
    }
}

pub struct Registry<P> {
    path: P,
    pub version_map: VersionMap,
}
pub type LoadError = misc_utils::json::ReadError;
pub const REGISTRY_PATH: &str = "library/three.js/version_map.json";
impl<P: AsRef<Path>> Registry<P> {
    pub fn load_or_default(path: P) -> Result<Self, LoadError> {
        let p = path.as_ref();
        if p.exists() {
            Ok(Self {
                version_map: misc_utils::json::read(p)?,
                path,
            })
        } else {
            Ok(Self {
                path,
                version_map: VersionMap::new(),
            })
        }
    }
    pub fn save(&self) -> Result<(), io::Error> {
        misc_utils::json::write(self.path.as_ref(), &self.version_map)
    }
}

pub const NPM_PACKAGE_NAME: &str = "three";

#[derive(Default)]
struct NoMissing;
impl<I> Extend<I> for NoMissing {
    fn extend<T: IntoIterator<Item = I>>(&mut self, _: T) {
        unreachable!()
    }
}

pub fn update_registries<F, P1, P2>(
    npm_reg: &mut npm_registry::Registry<P1>,
    registry: &mut Registry<P2>,
    versions: impl IntoIterator<Item = ThreeVersion>,
    meta: metadata::Package,
) -> Result<F, npm_registry::LoadError>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
    F: Default + Extend<ThreeVersion>,
{
    let info = npm_versions::<Vec<_>, Vec<_>, F>(versions, &meta);
    npm_reg
        .load_mut(String::from(NPM_PACKAGE_NAME))?
        .update_versions::<NoMissing>(NPM_PACKAGE_NAME, info.npm_info, meta);
    registry.version_map.extend(info.version_map);
    Ok(info.missing)
}
