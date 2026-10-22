//! A minimal HTTP/1.1 client for the one thing the release check needs:
//! `GET` a URL (following redirects) and return the body. `https` uses the
//! operating system's trust store.

use std::sync::Arc;

use quinn::rustls::crypto::ring;
use quinn::rustls::pki_types::ServerName;
use quinn::rustls::{ClientConfig, version::TLS13};
use rustls_platform_verifier::BuilderVerifierExt;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

/// GitHub answers a release asset with up to two redirects.
const MAX_REDIRECTS: usize = 5;

trait Io: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> Io for T {}

/// Where a URL points.
#[derive(Debug, PartialEq, Eq)]
struct Target {
    https: bool,
    host: String,
    port: u16,
    path: String,
}

impl Target {
    fn parse(url: &str) -> Result<Self, String> {
        let (https, rest) = if let Some(rest) = url.strip_prefix("https://") {
            (true, rest)
        } else if let Some(rest) = url.strip_prefix("http://") {
            (false, rest)
        } else {
            return Err(format!("{url} is not an http or https URL"));
        };
        let (authority, path) = match rest.find('/') {
            Some(at) => (&rest[..at], &rest[at..]),
            None => (rest, "/"),
        };
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) => (
                host,
                port.parse::<u16>()
                    .map_err(|_| format!("{url} has an invalid port"))?,
            ),
            None => (authority, if https { 443 } else { 80 }),
        };
        if host.is_empty() {
            return Err(format!("{url} has no host"));
        }
        Ok(Self {
            https,
            host: host.to_string(),
            port,
            path: path.to_string(),
        })
    }

    /// The URL a `Location` header leads to.
    fn follow(&self, location: &str) -> String {
        if location.contains("://") {
            location.to_string()
        } else {
            let scheme = if self.https { "https" } else { "http" };
            format!("{scheme}://{}:{}{location}", self.host, self.port)
        }
    }
}

struct Response {
    status: u16,
    location: Option<String>,
    body: Vec<u8>,
}

/// The HTTP client. Cheap to clone.
#[derive(Clone, Default)]
pub struct Http {
    /// TLS settings; the platform's trust store when `None`.
    tls: Option<Arc<ClientConfig>>,
}

impl Http {
    /// A client that trusts the operating system's certificate store.
    pub fn system() -> Self {
        Self::default()
    }

    /// A client with these TLS settings.
    pub fn with_tls(config: Arc<ClientConfig>) -> Self {
        Self { tls: Some(config) }
    }

    fn tls_config(&self) -> Arc<ClientConfig> {
        if let Some(config) = &self.tls {
            return config.clone();
        }
        let builder = ClientConfig::builder_with_provider(Arc::new(ring::default_provider()))
            .with_protocol_versions(&[&TLS13])
            .expect("TLS 1.3 is supported");
        let config = builder
            .with_platform_verifier()
            .expect("the platform certificate store is available")
            .with_no_client_auth();
        Arc::new(config)
    }

    /// The body of `url`, after following redirects; anything but a 200 is
    /// an error.
    pub async fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        let mut url = url.to_string();
        for _ in 0..=MAX_REDIRECTS {
            let target = Target::parse(&url)?;
            let response = self.once(&target).await?;
            match (response.status, response.location) {
                (200, _) => return Ok(response.body),
                (301 | 302 | 303 | 307 | 308, Some(location)) => url = target.follow(&location),
                (status, _) => return Err(format!("{url} answered {status}")),
            }
        }
        Err("too many redirects".into())
    }

    async fn once(&self, target: &Target) -> Result<Response, String> {
        let name = target
            .https
            .then(|| {
                ServerName::try_from(target.host.clone())
                    .map_err(|_| format!("{} is not a valid host name", target.host))
            })
            .transpose()?;
        let tcp = TcpStream::connect((target.host.as_str(), target.port))
            .await
            .map_err(|e| format!("cannot reach {}: {e}", target.host))?;
        let mut stream: Box<dyn Io> = if let Some(name) = name {
            let tls = TlsConnector::from(self.tls_config())
                .connect(name, tcp)
                .await
                .map_err(|e| format!("secure connection to {} failed: {e}", target.host))?;
            Box::new(tls)
        } else {
            Box::new(tcp)
        };
        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: mouser\r\nAccept: */*\r\nConnection: close\r\n\r\n",
            target.path, target.host
        );
        let mut raw = Vec::new();
        let exchange = async {
            stream.write_all(request.as_bytes()).await?;
            stream.read_to_end(&mut raw).await
        };
        exchange
            .await
            .map_err(|e| format!("{} dropped the connection: {e}", target.host))?;
        parse(&raw)
    }
}

fn parse(raw: &[u8]) -> Result<Response, String> {
    let malformed = || "malformed HTTP response".to_string();
    let end = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(malformed)?;
    let head = std::str::from_utf8(&raw[..end]).map_err(|_| malformed())?;
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(malformed)?;
    let (mut location, mut chunked, mut length) = (None, false, None);
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.to_ascii_lowercase().as_str() {
            "location" => location = Some(value.to_string()),
            "transfer-encoding" => chunked = value.eq_ignore_ascii_case("chunked"),
            "content-length" => length = value.parse::<usize>().ok(),
            _ => {}
        }
    }
    let body = &raw[end + 4..];
    let body = if chunked {
        dechunk(body).ok_or_else(malformed)?
    } else if let Some(length) = length {
        body.get(..length).ok_or_else(malformed)?.to_vec()
    } else {
        body.to_vec()
    };
    Ok(Response {
        status,
        location,
        body,
    })
}

fn dechunk(mut body: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let line_end = body.windows(2).position(|w| w == b"\r\n")?;
        let size = std::str::from_utf8(&body[..line_end]).ok()?;
        let size = usize::from_str_radix(size.split(';').next()?.trim(), 16).ok()?;
        body = &body[line_end + 2..];
        if size == 0 {
            return Some(out);
        }
        out.extend(body.get(..size)?);
        body = body.get(size + 2..)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_split_into_host_port_and_path() {
        let t = Target::parse("https://api.github.com/repos/x?per_page=3").unwrap();
        assert_eq!(
            (t.https, t.host.as_str(), t.port, t.path.as_str()),
            (true, "api.github.com", 443, "/repos/x?per_page=3")
        );
        let t = Target::parse("http://localhost:8080").unwrap();
        assert_eq!(
            (t.https, t.host.as_str(), t.port, t.path.as_str()),
            (false, "localhost", 8080, "/")
        );
        assert!(
            Target::parse("ftp://x/y")
                .unwrap_err()
                .contains("not an http")
        );
        assert!(
            Target::parse("http://x:port/")
                .unwrap_err()
                .contains("invalid port")
        );
        assert!(Target::parse("http:///x").unwrap_err().contains("no host"));
    }

    #[test]
    fn locations_resolve_against_the_current_url() {
        let t = Target::parse("http://h:81/a").unwrap();
        assert_eq!(t.follow("/b"), "http://h:81/b");
        assert_eq!(t.follow("https://o/c"), "https://o/c");
        let t = Target::parse("https://h/a").unwrap();
        assert_eq!(t.follow("/b"), "https://h:443/b");
    }
}
