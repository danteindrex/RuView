use rcgen::CertifiedKey;
use std::path::Path;
use tokio::fs;

pub async fn load_or_generate_cert(cert_dir: &Path) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let cert_path = cert_dir.join("server.crt");
    let key_path = cert_dir.join("server.key");
    if cert_path.exists() && key_path.exists() {
        return Ok((fs::read(&cert_path).await?, fs::read(&key_path).await?));
    }
    fs::create_dir_all(cert_dir).await?;
    let CertifiedKey { cert, key_pair } =
        rcgen::generate_simple_self_signed(vec!["localhost".to_string(), "127.0.0.1".to_string()])?;
    let cert_pem = cert.pem().into_bytes();
    let key_pem = key_pair.serialize_pem().into_bytes();
    fs::write(&cert_path, &cert_pem).await?;
    fs::write(&key_path, &key_pem).await?;
    tracing::info!("Generated self-signed TLS cert at {}", cert_dir.display());
    Ok((cert_pem, key_pem))
}
