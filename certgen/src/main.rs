use std::{
    borrow::Cow,
    env, error,
    fmt::Display,
    fs,
    io::{self, Write},
    ops::Add,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use local_cdn_certgen::generate;
use serde::{Deserialize, Serialize};
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

#[derive(Serialize, Deserialize)]
struct State {
    expire: SystemTime,
    #[serde(with = "const_hex::serde")]
    config_sha256: [u8; 32],
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Overwrite {
    Always,
    Expired,
    Never,
}
#[derive(Deserialize)]
struct Config {
    overwrite: Overwrite,
    cert: local_cdn_certgen::Config,
}

fn write_with_perm(
    path: impl AsRef<Path>,
    content: impl AsRef<[u8]>,
    mode: u32,
) -> Result<(), io::Error> {
    let mut f = fs::File::options()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .open(path)?;
    f.write_all(content.as_ref())?;
    Ok(())
}

fn overwrite(
    state_path: &Path,
    ca_path: &Path,
    config: &Config,
    time: SystemTime,
    config_sha256: &[u8; 32],
) -> Result<bool, Error> {
    let old_state = if state_path.exists() {
        serde_json::from_slice::<State>(
            &fs::read(&state_path).context("failed to read state file")?,
        )
        .context("failed to parse state file")?
    } else {
        return Ok(true);
    };
    if &old_state.config_sha256 != config_sha256 || !ca_path.exists() {
        return Ok(true);
    }
    Ok(match config.overwrite {
        Overwrite::Always => true,
        Overwrite::Expired => old_state.expire <= time,
        Overwrite::Never => false,
    })
}

fn main() -> Result<(), Error> {
    let (config_path, ca_path, servers_path, state_path) = {
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
            PathBuf::from(args.next().ok_or_else(|| Error {
                message: Cow::Borrowed("missing state path"),
                inner: None,
            })?),
        )
    };
    let config_data = fs::read(config_path).context("failed to read config file")?;
    let config: Config =
        serde_json::from_slice(&config_data).context("failed to parse config file")?;
    let config_sha256 = {
        use sha2::Digest;
        sha2::Sha256::digest(&config_data).into()
    };

    let ca_path = {
        let mut p = PathBuf::from(ca_path);
        if !p.exists() {
            fs::create_dir(&p).context("failed to create ca directory")?;
        }
        p.push(format!("{}.pem", config.cert.ca_name));
        p
    };

    let time = SystemTime::now();

    if !overwrite(&state_path, &ca_path, &config, time, &config_sha256)? {
        println!("skipped generate new certificate");
        return Ok(());
    }
    let state = State {
        expire: time.add(Duration::from_secs(config.cert.expire_secs.get() as u64)),
        config_sha256,
    };

    let (mut ca, servers) = generate(config.cert, time::OffsetDateTime::from(time))
        .context("failed to generate certificate")?;

    fs::write(ca_path, ca.certified_key.cert.pem()).context("failed to write ca cert")?;
    ca.certified_key.key_pair.zeroize();

    let servers_path = PathBuf::from(servers_path);
    fs::create_dir_all(&servers_path).context("failed to create servers directory")?;
    for (idx, mut s) in servers.into_iter().enumerate() {
        let mut p = servers_path.join(format!("{}.key", s.name));
        write_with_perm(&p, s.certified_key.key_pair.serialize_pem(), 0o600)
            .with_context(|| format!("failed to write server {} key", idx))?;
        s.certified_key.key_pair.zeroize();

        p.set_extension("pem");
        fs::write(&p, s.certified_key.cert.pem())
            .with_context(|| format!("failed to write server {} cert", idx))?;
    }

    fs::write(&state_path, serde_json::to_vec(&state).unwrap())
        .context("failed to write state file")?;

    Ok(())
}
