use std::collections;

pub fn to_drv_name(s: &str) -> String {
    s.replace(
        |c| match c {
            '+' | '.' | '_' | '?' | '=' | '-' => false,
            c => !c.is_ascii_alphanumeric(),
        },
        "_",
    )
}

pub trait IsEmpty {
    fn is_empty(&self) -> bool;
}
impl<T> IsEmpty for Vec<T> {
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}
impl<K, V> IsEmpty for collections::HashMap<K, V> {
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}
impl<K> IsEmpty for collections::HashSet<K> {
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}
impl<K, V> IsEmpty for collections::BTreeMap<K, V> {
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}
impl<K> IsEmpty for collections::BTreeSet<K> {
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}
impl<T> IsEmpty for collections::LinkedList<T> {
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}
impl<T> IsEmpty for [T] {
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}

pub mod json {
    use std::{fs, io, path::Path};

    use serde::{de::DeserializeOwned, Serialize};

    #[derive(Debug, thiserror::Error)]
    pub enum ReadError {
        #[error("failed to read file")]
        ReadFile(#[source] io::Error),
        #[error("failed to parse json")]
        ParseJson(#[source] serde_json::Error),
    }
    pub fn read<S: DeserializeOwned>(path: impl AsRef<Path>) -> Result<S, ReadError> {
        serde_json::from_slice(&fs::read(path).map_err(ReadError::ReadFile)?)
            .map_err(ReadError::ParseJson)
    }

    pub fn write<S: Serialize>(path: impl AsRef<Path>, value: &S) -> Result<(), io::Error> {
        fs::write(path, serde_json::to_vec_pretty(value).unwrap())
    }
}
