use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};

use crate::{
    config::{ScanDecision, ScannerAdapterConfig, ScanningConfig},
    util,
};

#[derive(Debug, Clone, Serialize)]
pub struct ScanReport {
    pub adapter: String,
    pub decision: ScanDecision,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanSummary {
    pub decision: ScanDecision,
    pub reports: Vec<ScanReport>,
}

#[derive(Debug, Clone)]
pub struct ScanInput<'a> {
    pub bytes: &'a Bytes,
    pub filename: Option<&'a str>,
    pub content_type: Option<&'a str>,
    pub hash: &'a str,
    pub public_id: &'a str,
}

pub async fn scan_upload(config: &ScanningConfig, input: ScanInput<'_>) -> ScanSummary {
    if !config.enabled || config.adapters.is_empty() {
        return ScanSummary {
            decision: ScanDecision::Allow,
            reports: Vec::new(),
        };
    }

    let mut reports = Vec::new();
    for adapter in &config.adapters {
        let report = match adapter {
            ScannerAdapterConfig::Command { program, args } => {
                run_command_scanner(program, args, &input, config.default_on_error).await
            }
            ScannerAdapterConfig::Webhook { url, secret } => {
                run_webhook_scanner(url, secret.as_deref(), &input, config.default_on_error).await
            }
            ScannerAdapterConfig::ClamAv { socket } => {
                run_clamav_scanner(socket, &input, config.default_on_error).await
            }
        };
        reports.push(report);
    }

    let decision = reports
        .iter()
        .map(|report| report.decision)
        .max_by_key(decision_rank)
        .unwrap_or(ScanDecision::Allow);

    ScanSummary { decision, reports }
}

async fn run_command_scanner(
    program: &str,
    args: &[String],
    input: &ScanInput<'_>,
    default_on_error: ScanDecision,
) -> ScanReport {
    let temp_path = scanner_temp_path(input.public_id);
    let result = async {
        tokio::fs::write(&temp_path, input.bytes).await?;
        let mut command = Command::new(program);
        for arg in args {
            command.arg(expand_arg(arg, input, &temp_path));
        }
        let output = command.output().await?;
        let _ = tokio::fs::remove_file(&temp_path).await;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = format!(
            "status={:?}; stdout={}; stderr={}",
            output.status.code(),
            stdout.trim(),
            stderr.trim()
        );
        let decision = match output.status.code() {
            Some(0) => ScanDecision::Allow,
            Some(10) => ScanDecision::Quarantine,
            Some(20) => ScanDecision::Reject,
            _ => default_on_error,
        };
        anyhow::Ok((decision, detail))
    }
    .await;

    match result {
        Ok((decision, detail)) => ScanReport {
            adapter: "command".to_string(),
            decision,
            detail,
        },
        Err(err) => ScanReport {
            adapter: "command".to_string(),
            decision: default_on_error,
            detail: format!("scanner command failed: {err}"),
        },
    }
}

#[derive(Debug, Serialize)]
struct WebhookScanRequest<'a> {
    filename: Option<&'a str>,
    content_type: Option<&'a str>,
    size_bytes: usize,
    sha256: &'a str,
    public_id: &'a str,
}

#[derive(Debug, Deserialize)]
struct WebhookScanResponse {
    decision: ScanDecision,
    detail: Option<String>,
}

async fn run_webhook_scanner(
    url: &str,
    secret: Option<&str>,
    input: &ScanInput<'_>,
    default_on_error: ScanDecision,
) -> ScanReport {
    let client = reqwest::Client::new();
    let mut request = client.post(url).json(&WebhookScanRequest {
        filename: input.filename,
        content_type: input.content_type,
        size_bytes: input.bytes.len(),
        sha256: input.hash,
        public_id: input.public_id,
    });
    if let Some(secret) = secret {
        request = request.header("x-midden-scanner-secret", secret);
    }
    match request.send().await {
        Ok(response) if response.status().is_success() => {
            match response.json::<WebhookScanResponse>().await {
                Ok(decoded) => ScanReport {
                    adapter: "webhook".to_string(),
                    decision: decoded.decision,
                    detail: decoded
                        .detail
                        .unwrap_or_else(|| "webhook completed".to_string()),
                },
                Err(err) => ScanReport {
                    adapter: "webhook".to_string(),
                    decision: default_on_error,
                    detail: format!("invalid webhook response: {err}"),
                },
            }
        }
        Ok(response) => ScanReport {
            adapter: "webhook".to_string(),
            decision: default_on_error,
            detail: format!("webhook returned HTTP {}", response.status()),
        },
        Err(err) => ScanReport {
            adapter: "webhook".to_string(),
            decision: default_on_error,
            detail: format!("webhook failed: {err}"),
        },
    }
}

async fn run_clamav_scanner(
    socket: &str,
    input: &ScanInput<'_>,
    default_on_error: ScanDecision,
) -> ScanReport {
    let result = if socket.contains(':') {
        scan_clamav_tcp(socket, input.bytes).await
    } else {
        scan_clamav_unix(socket, input.bytes).await
    };

    match result {
        Ok(detail) => {
            let decision = if detail.contains("FOUND") {
                ScanDecision::Reject
            } else if detail.contains("OK") {
                ScanDecision::Allow
            } else {
                default_on_error
            };
            ScanReport {
                adapter: "clamav".to_string(),
                decision,
                detail,
            }
        }
        Err(err) => ScanReport {
            adapter: "clamav".to_string(),
            decision: default_on_error,
            detail: format!("clamav scan failed: {err}"),
        },
    }
}

async fn scan_clamav_tcp(addr: &str, bytes: &Bytes) -> anyhow::Result<String> {
    let mut stream = tokio::net::TcpStream::connect(addr).await?;
    write_clamav_instream(&mut stream, bytes).await?;
    read_clamav_response(&mut stream).await
}

#[cfg(unix)]
async fn scan_clamav_unix(path: &str, bytes: &Bytes) -> anyhow::Result<String> {
    let mut stream = tokio::net::UnixStream::connect(path).await?;
    write_clamav_instream(&mut stream, bytes).await?;
    read_clamav_response(&mut stream).await
}

#[cfg(not(unix))]
async fn scan_clamav_unix(_path: &str, _bytes: &Bytes) -> anyhow::Result<String> {
    anyhow::bail!("unix ClamAV sockets are not supported on this platform")
}

async fn write_clamav_instream<W>(stream: &mut W, bytes: &Bytes) -> anyhow::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    stream.write_all(b"zINSTREAM\0").await?;
    for chunk in bytes.chunks(1024 * 1024) {
        stream
            .write_all(&(chunk.len() as u32).to_be_bytes())
            .await?;
        stream.write_all(chunk).await?;
    }
    stream.write_all(&0_u32.to_be_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

async fn read_clamav_response<R>(stream: &mut R) -> anyhow::Result<String>
where
    R: AsyncReadExt + Unpin,
{
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await?;
    Ok(String::from_utf8_lossy(&response).trim().to_string())
}

fn scanner_temp_path(public_id: &str) -> PathBuf {
    std::env::temp_dir().join(format!("midden-scan-{public_id}-{}", util::public_id()))
}

fn expand_arg(arg: &str, input: &ScanInput<'_>, path: &Path) -> String {
    let values = BTreeMap::from([
        ("path", path.to_string_lossy().to_string()),
        ("filename", input.filename.unwrap_or("").to_string()),
        ("content_type", input.content_type.unwrap_or("").to_string()),
        ("sha256", input.hash.to_string()),
        ("public_id", input.public_id.to_string()),
    ]);
    values.iter().fold(arg.to_string(), |acc, (key, value)| {
        acc.replace(&format!("{{{key}}}"), value)
    })
}

fn decision_rank(decision: &ScanDecision) -> u8 {
    match decision {
        ScanDecision::Allow => 0,
        ScanDecision::Quarantine => 1,
        ScanDecision::Reject => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn disabled_scanning_allows_without_reports() {
        let config = ScanningConfig::default();
        let bytes = Bytes::from_static(b"hello");
        let summary = scan_upload(
            &config,
            ScanInput {
                bytes: &bytes,
                filename: Some("hello.txt"),
                content_type: Some("text/plain"),
                hash: "abc",
                public_id: "id",
            },
        )
        .await;
        assert_eq!(summary.decision, ScanDecision::Allow);
        assert!(summary.reports.is_empty());
    }
}
