use std::{
    borrow::Cow,
    env, error,
    fmt::Display,
    fs::{self, Permissions},
    io::{self, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use local_cdn_certgen::generate;
use zeroize::Zeroize;

#[derive(Debug)]
struct Error {
    message: Cow<'static, str>,
    inner: Option<Box<dyn error::Error + 'static>>,
}
impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.inner {
            Some(e) => write!(f, "{}: {}", self.message, e),
            None => f.write_str(&self.message),
        }
    }
}
impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        self.inner.as_ref().map(Box::as_ref)
    }
}

trait ResultExt<T, E> {
    fn context(self, message: &'static str) -> Result<T, Error>;
    fn with_context(self, f: impl FnOnce() -> String) -> Result<T, Error>;
}
impl<T, E: error::Error + 'static> ResultExt<T, E> for Result<T, E> {
    fn context(self, message: &'static str) -> Result<T, Error> {
        match self {
            Ok(r) => Ok(r),
            Err(e) => Err(Error {
                message: Cow::Borrowed(message),
                inner: Some(Box::new(e)),
            }),
        }
    }
    fn with_context(self, f: impl FnOnce() -> String) -> Result<T, Error> {
        match self {
            Ok(r) => Ok(r),
            Err(e) => Err(Error {
                message: Cow::Owned(f()),
                inner: Some(Box::new(e)),
            }),
        }
    }
}

fn write_with_perm(
    path: impl AsRef<Path>,
    content: impl AsRef<[u8]>,
    perm: Permissions,
) -> Result<(), io::Error> {
    let mut f = fs::File::create(path)?;
    f.write_all(content.as_ref())?;
    f.set_permissions(perm)?;
    Ok(())
}

fn main() -> Result<(), Error> {
    let (config_path, ca_path, servers_path) = {
        let mut args = env::args_os().fuse();
        args.next().ok_or_else(|| Error {
            message: Cow::Borrowed("missing program name"),
            inner: None,
        })?;
        (
            args.next().ok_or_else(|| Error {
                message: Cow::Borrowed("missing config file path"),
                inner: None,
            })?,
            args.next().ok_or_else(|| Error {
                message: Cow::Borrowed("missing ca path"),
                inner: None,
            })?,
            args.next().ok_or_else(|| Error {
                message: Cow::Borrowed("missing server path"),
                inner: None,
            })?,
        )
    };
    let config =
        serde_json::from_slice(&fs::read(config_path).context("failed to read config file")?)
            .context("failed to parse config file")?;

    let (mut ca, servers) = generate(config, time::OffsetDateTime::now_utc())
        .context("failed to generate certificate")?;

    fs::write(
        {
            let mut p = PathBuf::from(ca_path);
            fs::create_dir_all(&p).context("failed to create ca directory")?;
            p.push(format!("{}.pem", ca.name));
            p
        },
        ca.certified_key.cert.pem(),
    )
    .context("failed to write ca cert")?;
    ca.certified_key.key_pair.zeroize();

    let servers_path = PathBuf::from(servers_path);
    fs::create_dir_all(&servers_path).context("failed to create servers directory")?;
    for (idx, mut s) in servers.into_iter().enumerate() {
        let mut p = servers_path.join(format!("{}.key", s.name));
        write_with_perm(
            &p,
            s.certified_key.key_pair.serialize_pem(),
            Permissions::from_mode(0o600),
        )
        .with_context(|| format!("failed to write server {} key", idx))?;
        s.certified_key.key_pair.zeroize();

        p.set_extension("pem");
        fs::write(&p, s.certified_key.cert.pem())
            .with_context(|| format!("failed to write server {} cert", idx))?;
    }

    Ok(())
}
