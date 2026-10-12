//! Certificados TLS locais, gerados em Rust puro.
//!
//! Uma CA própria ("MicBridge Local CA", instalada uma vez no celular) assina o
//! certificado do servidor, que cobre o IP da máquina e `micbridge.local`. A chave
//! da CA nunca sai deste PC. O certificado é refeito se o IP mudar ou estiver
//! perto de vencer; a CA é preservada, então os celulares continuam confiando.

use crate::state::data_dir;
use anyhow::{Context, Result};
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose, SanType,
};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::net::IpAddr;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use time::{Duration, OffsetDateTime};

/// Nome mDNS incluído no certificado.
pub const MDNS_HOST: &str = "micbridge.local";

const CA_CN: &str = "MicBridge Local CA";
const CA_VALID_DAYS: i64 = 3650;
/// Navegadores rejeitam certificados de servidor com mais de 398 dias.
const SERVER_VALID_DAYS: i64 = 397;
/// Refaz o certificado do servidor depois deste tempo.
const SERVER_RENEW_DAYS: u64 = 300;

pub struct Tls {
    pub cert_pem: String,
    pub key_pem: String,
}

pub fn certs_dir() -> PathBuf {
    data_dir().join("certs")
}

/// Caminho do `ca.pem`, entregue ao celular em `/ca.crt`.
pub fn ca_path() -> PathBuf {
    certs_dir().join("ca.pem")
}

/// SHA-256 do certificado do servidor (formato `AA:BB:…`), para conferir no celular.
pub fn fingerprint() -> Option<String> {
    let text = std::fs::read_to_string(certs_dir().join("cert.sha256")).ok()?;
    Some(text.trim().to_string())
}

fn ca_params() -> Result<CertificateParams> {
    let mut p = CertificateParams::new(Vec::<String>::new())?;
    p.distinguished_name.push(DnType::CommonName, CA_CN);
    p.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    p.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    Ok(p)
}

/// Escrita privada (0600) para chaves; 0644 para o que o celular pode baixar.
fn write(path: &Path, content: &str, mode: u32) -> Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .open(path)
        .with_context(|| format!("gravando {}", path.display()))?;
    f.write_all(content.as_bytes())?;
    Ok(())
}

fn colon_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn age_days(path: &Path) -> Option<u64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(modified.elapsed().ok()?.as_secs() / 86_400)
}

/// Garante CA e certificado do servidor válidos para `ip`, criando o que faltar.
pub fn ensure(ip: Option<IpAddr>) -> Result<Tls> {
    let dir = certs_dir();
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)?;

    let now = OffsetDateTime::now_utc();
    let (ca_key_path, ca_pem_path) = (dir.join("ca.key"), dir.join("ca.pem"));

    // CA: criada uma única vez.
    let ca_key = if ca_key_path.exists() && ca_pem_path.exists() {
        KeyPair::from_pem(&std::fs::read_to_string(&ca_key_path)?)?
    } else {
        let key = KeyPair::generate()?;
        let mut p = ca_params()?;
        p.not_before = now - Duration::days(1);
        p.not_after = now + Duration::days(CA_VALID_DAYS);
        let cert = p.self_signed(&key)?;
        write(&ca_key_path, &key.serialize_pem(), 0o600)?;
        write(&ca_pem_path, &cert.pem(), 0o644)?;
        tracing::info!("CA local criada em {}", dir.display());
        key
    };

    // Servidor: reaproveita se o IP é o mesmo e ainda não está perto de vencer.
    let (cert_path, key_path) = (dir.join("cert.pem"), dir.join("key.pem"));
    let (ip_path, fp_path) = (dir.join("server.ip"), dir.join("cert.sha256"));
    let ip_text = ip.map(|i| i.to_string()).unwrap_or_default();
    let reusable = cert_path.exists()
        && key_path.exists()
        && fp_path.exists()
        && std::fs::read_to_string(&ip_path).ok().as_deref() == Some(ip_text.as_str())
        && age_days(&cert_path).is_some_and(|d| d < SERVER_RENEW_DAYS);
    if reusable {
        return Ok(Tls {
            cert_pem: std::fs::read_to_string(&cert_path)?,
            key_pem: std::fs::read_to_string(&key_path)?,
        });
    }

    let issuer = Issuer::new(ca_params()?, ca_key);
    let mut p = CertificateParams::new(vec![MDNS_HOST.to_string(), "localhost".to_string()])?;
    p.distinguished_name.push(DnType::CommonName, MDNS_HOST);
    p.subject_alt_names
        .push(SanType::IpAddress(IpAddr::from([127, 0, 0, 1])));
    if let Some(ip) = ip {
        p.subject_alt_names.push(SanType::IpAddress(ip));
    }
    p.is_ca = IsCa::ExplicitNoCa;
    p.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    p.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    p.not_before = now - Duration::days(1);
    p.not_after = now + Duration::days(SERVER_VALID_DAYS);

    let key = KeyPair::generate()?;
    let cert = p.signed_by(&key, &issuer)?;
    let (cert_pem, key_pem) = (cert.pem(), key.serialize_pem());

    write(&key_path, &key_pem, 0o600)?;
    write(&cert_path, &cert_pem, 0o644)?;
    write(&ip_path, &ip_text, 0o644)?;
    write(&fp_path, &colon_hex(&Sha256::digest(cert.der())), 0o644)?;
    tracing::info!("certificado do servidor gerado para {ip_text}");

    Ok(Tls { cert_pem, key_pem })
}
