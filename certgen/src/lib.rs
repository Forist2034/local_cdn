use std::{collections::HashMap, fmt::Display, net::IpAddr, num::NonZeroU32};

use rcgen::{
    CertificateParams, CertifiedKey, DnType, ExtendedKeyUsagePurpose, Ia5String, IsCa, KeyPair,
    KeyUsagePurpose, SanType, SerialNumber, SignatureAlgorithm,
};
use serde::Deserialize;
use time::OffsetDateTime;

#[derive(Debug)]
pub struct Ia5Wrapper(Ia5String);
impl<'de> Deserialize<'de> for Ia5Wrapper {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Ia5Wrapper;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("ASN.1 IA5String")
            }
            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v.parse() {
                    Ok(s) => Ok(Ia5Wrapper(s)),
                    Err(e) => Err(E::custom(e)),
                }
            }
            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match Ia5String::try_from(v) {
                    Ok(s) => Ok(Ia5Wrapper(s)),
                    Err(e) => Err(E::custom(e)),
                }
            }
        }
        deserializer.deserialize_string(Visitor)
    }
}

#[derive(Debug, Deserialize)]
pub struct SubjectAltNames {
    #[serde(default)]
    pub dns: Vec<Ia5Wrapper>,
    #[serde(default)]
    pub ip_addr: Vec<IpAddr>,
}

#[derive(Debug, Deserialize)]
pub struct DistinguishedName {
    pub organization_unit_name: String,
    pub common_name: String,
}

#[derive(Debug, Deserialize)]
pub struct CertConfig {
    pub distinguished_name: DistinguishedName,
    pub subject_alt_names: SubjectAltNames,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub organization_name: String,
    pub expire_secs: NonZeroU32,
    pub ca_name: String,
    pub ca: CertConfig,
    pub servers: HashMap<String, CertConfig>,
}

#[derive(Debug, Clone, Copy)]
enum ErrCert {
    CA,
    Server(usize),
}

#[derive(Debug)]
enum InnerError {
    GenKeyPair(rcgen::Error),
    GenSerial(getrandom::Error),
    Sign(rcgen::Error),
}

#[derive(Debug)]
pub struct Error {
    cert: ErrCert,
    inner: InnerError,
}
impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("failed to generate ")?;
        match self.cert {
            ErrCert::CA => f.write_str("ca root cert")?,
            ErrCert::Server(s) => write!(f, " server {} cert", s)?,
        }
        f.write_str(": ")?;
        match &self.inner {
            InnerError::GenKeyPair(e) => write!(f, "failed to generate key pair: {}", e),
            InnerError::GenSerial(e) => write!(f, "failed to generate serial: {}", e),
            InnerError::Sign(e) => write!(f, "failed to sign certificate: {}", e),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.inner {
            InnerError::GenKeyPair(e) => Some(e),
            InnerError::GenSerial(_) => None,
            InnerError::Sign(e) => Some(e),
        }
    }
}

pub struct NamedCert {
    pub name: String,
    pub certified_key: CertifiedKey,
}

struct GenInfo<'a> {
    not_before: OffsetDateTime,
    not_after: OffsetDateTime,
    organization_name: &'a str,
}

fn to_cert_param(config: CertConfig, info: &GenInfo<'_>) -> Result<CertificateParams, InnerError> {
    let mut ret = CertificateParams::default();
    ret.distinguished_name
        .push(DnType::OrganizationName, info.organization_name);
    ret.distinguished_name.push(
        DnType::OrganizationalUnitName,
        config.distinguished_name.organization_unit_name,
    );
    ret.distinguished_name
        .push(DnType::CommonName, config.distinguished_name.common_name);

    ret.not_before = info.not_before;
    ret.not_after = info.not_after;

    for d in config.subject_alt_names.dns {
        ret.subject_alt_names.push(SanType::DnsName(d.0))
    }
    for a in config.subject_alt_names.ip_addr {
        ret.subject_alt_names.push(SanType::IpAddress(a))
    }

    ret.serial_number = Some({
        let mut ret = [0; 20];
        getrandom::getrandom(&mut ret).map_err(InnerError::GenSerial)?;
        SerialNumber::from_slice(&ret)
    });

    Ok(ret)
}

static SIG_ALGO: &SignatureAlgorithm = &rcgen::PKCS_RSA_SHA256;

fn generate_ca(config: CertConfig, info: &GenInfo<'_>) -> Result<CertifiedKey, InnerError> {
    let key_pair = KeyPair::generate_for(SIG_ALGO).map_err(InnerError::GenKeyPair)?;
    let mut param = to_cert_param(config, info)?;
    param.key_usages.push(KeyUsagePurpose::KeyCertSign);
    param.is_ca = IsCa::Ca(rcgen::BasicConstraints::Constrained(0));

    Ok(CertifiedKey {
        cert: param.self_signed(&key_pair).map_err(InnerError::Sign)?,
        key_pair,
    })
}

fn gen_server_cert(
    config: CertConfig,
    info: &GenInfo<'_>,
    ca: &CertifiedKey,
) -> Result<CertifiedKey, InnerError> {
    let key_pair = KeyPair::generate_for(SIG_ALGO).map_err(InnerError::GenKeyPair)?;
    let mut param = to_cert_param(config, info)?;
    param.key_usages.push(KeyUsagePurpose::DigitalSignature);
    param
        .extended_key_usages
        .push(ExtendedKeyUsagePurpose::ServerAuth);
    Ok(CertifiedKey {
        cert: param
            .signed_by(&key_pair, &ca.cert, &ca.key_pair)
            .map_err(InnerError::Sign)?,
        key_pair,
    })
}

pub fn generate(
    config: Config,
    not_before: time::OffsetDateTime,
) -> Result<(NamedCert, Vec<NamedCert>), Error> {
    let info = GenInfo {
        not_before,
        not_after: not_before + time::Duration::seconds(config.expire_secs.get() as i64),
        organization_name: &config.organization_name,
    };
    let ca = generate_ca(config.ca, &info).map_err(|inner| Error {
        cert: ErrCert::CA,
        inner,
    })?;
    let mut certs = Vec::with_capacity(config.servers.len());
    for (idx, (name, cfg)) in config.servers.into_iter().enumerate() {
        certs.push(NamedCert {
            name,
            certified_key: gen_server_cert(cfg, &info, &ca).map_err(|inner| Error {
                cert: ErrCert::Server(idx),
                inner,
            })?,
        });
    }
    Ok((
        NamedCert {
            name: config.ca_name,
            certified_key: ca,
        },
        certs,
    ))
}
