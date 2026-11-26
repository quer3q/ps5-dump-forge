//! GETs from Sony's game-update servers for metadata recovery. `sgst.prod.dl.playstation.net`
//! chains to "SCEI DNAS Root 05" (no public store carries it) with RSA/SHA-1, which rustls
//! refuses, so a `*.playstation.net` host is verified against that root with SHA-1 accepted in the
//! chain, then against the public roots; other hosts only the latter. Redirects are followed by
//! hand so HTTPS never goes on to HTTP.

use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use rustls::pki_types::{AlgorithmIdentifier, InvalidSignature, SignatureVerificationAlgorithm};

const SONY_ROOT: &[u8] = include_bytes!("scei-dnas-root-05.der");
const MAX_REDIRECTS: usize = 5;
const CONNECT_TIME: Duration = Duration::from_secs(15);
const READ_TIME: Duration = Duration::from_secs(60);

/// `sha1WithRSAEncryption` (1.2.840.113549.1.1.5, NULL parameters).
const RSA_PKCS1_SHA1: AlgorithmIdentifier = AlgorithmIdentifier::from_slice(&[
    0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x05, 0x05, 0x00,
]);
/// `rsaEncryption` with NULL parameters.
const RSA_ENCRYPTION: AlgorithmIdentifier = AlgorithmIdentifier::from_slice(&[
    0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01, 0x05, 0x00,
]);

/// RSA PKCS#1 SHA-1, 2048 bits and up: in a `*.playstation.net` chain only, never the handshake.
#[derive(Debug)]
struct RsaSha1;

impl SignatureVerificationAlgorithm for RsaSha1 {
    fn verify_signature(
        &self,
        key: &[u8],
        message: &[u8],
        sig: &[u8],
    ) -> Result<(), InvalidSignature> {
        let alg = &ring::signature::RSA_PKCS1_2048_8192_SHA1_FOR_LEGACY_USE_ONLY;
        ring::signature::UnparsedPublicKey::new(alg, key)
            .verify(message, sig)
            .map_err(|_| InvalidSignature)
    }
    fn public_key_alg_id(&self) -> AlgorithmIdentifier {
        RSA_ENCRYPTION
    }
    fn signature_alg_id(&self) -> AlgorithmIdentifier {
        RSA_PKCS1_SHA1
    }
}

fn is_sony(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host == "playstation.net" || host.ends_with(".playstation.net")
}

fn agent(sony: bool) -> ureq::Agent {
    static SONY: OnceLock<ureq::Agent> = OnceLock::new();
    static PLAIN: OnceLock<ureq::Agent> = OnceLock::new();
    let base = || {
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_connect(Some(CONNECT_TIME))
            .timeout_per_read(Some(READ_TIME))
    };
    if !sony {
        return PLAIN.get_or_init(|| base().build().into()).clone();
    }
    SONY.get_or_init(|| {
        let ring = rustls::crypto::ring::default_provider();
        let mut all = ring.signature_verification_algorithms.all.to_vec();
        all.push(&RsaSha1);
        let provider = rustls::crypto::CryptoProvider {
            signature_verification_algorithms: rustls::crypto::WebPkiSupportedAlgorithms {
                all: Box::leak(all.into_boxed_slice()),
                mapping: ring.signature_verification_algorithms.mapping,
            },
            ..ring
        };
        let root = ureq::tls::Certificate::from_der(SONY_ROOT);
        let tls = ureq::tls::TlsConfig::builder()
            .root_certs(ureq::tls::RootCerts::new_with_certs(&[root]))
            .unversioned_rustls_crypto_provider(Arc::new(provider))
            .build();
        base().tls_config(tls).build().into()
    })
    .clone()
}

fn call(url: &str, host: &str) -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
    if !is_sony(host) {
        return agent(false).get(url).call();
    }
    match agent(true).get(url).call() {
        Err(ureq::Error::Rustls(_)) => agent(false).get(url).call(),
        other => other,
    }
}

/// The body at `url` (no credentials in it), at most `max` bytes. `progress(got, expected)`,
/// `expected` 0 without a Content-Length; `cancel` stops it between reads (`Interrupted`).
pub fn get(
    url: &str,
    max: u64,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> io::Result<Vec<u8>> {
    let bad = |why: String| io::Error::new(io::ErrorKind::InvalidInput, why);
    let mut url = url.trim().to_string();
    let mut was_https = false;
    for _ in 0..=MAX_REDIRECTS {
        let uri: ureq::http::Uri = url
            .parse()
            .map_err(|_| bad(format!("{url} is not a URL")))?;
        let https = match uri.scheme_str() {
            Some("https") => true,
            Some("http") => false,
            _ => {
                return Err(bad(format!(
                    "{url}: only http:// and https:// URLs are fetched"
                )));
            }
        };
        if was_https && !https {
            return Err(bad(format!(
                "an HTTPS address redirected to plain HTTP: {url}"
            )));
        }
        was_https = https;
        let authority = uri.authority().map(|a| a.as_str()).unwrap_or("");
        if authority.contains('@') {
            return Err(bad(format!(
                "{url}: a URL with credentials in it is not fetched"
            )));
        }
        let host = uri.host().unwrap_or("");
        let reply = call(&url, host).map_err(|e| io::Error::other(format!("{url}: {e}")))?;
        let status = reply.status().as_u16();
        if (300..400).contains(&status) {
            let to = reply
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| io::Error::other(format!("{url}: a redirect with no Location")))?;
            url = resolve(&uri, to);
            continue;
        }
        if status != 200 {
            return Err(io::Error::other(format!(
                "{url}: the server answered {status}"
            )));
        }
        let expected = reply
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok());
        if expected.is_some_and(|n| n > max) {
            return Err(io::Error::other(format!(
                "{url}: {} bytes, over the {max} taken",
                expected.unwrap_or(0)
            )));
        }
        let mut body = reply.into_body().into_with_config().limit(max + 1).reader();
        let mut out = Vec::new();
        let mut buf = vec![0u8; 64 << 10];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
            }
            let n = match body.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(io::Error::new(e.kind(), format!("{url}: {e}"))),
            };
            if out.len() as u64 + n as u64 > max {
                return Err(io::Error::other(format!(
                    "{url}: longer than the {max} bytes taken"
                )));
            }
            out.extend_from_slice(&buf[..n]);
            progress(out.len() as u64, expected.unwrap_or(0));
        }
        if expected.is_some_and(|n| n != out.len() as u64) {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "{url}: the download ended after {} of {} bytes",
                    out.len(),
                    expected.unwrap_or(0)
                ),
            ));
        }
        return Ok(out);
    }
    Err(io::Error::other(format!(
        "{url}: more than {MAX_REDIRECTS} redirects"
    )))
}

fn resolve(from: &ureq::http::Uri, to: &str) -> String {
    if to.contains("://") {
        return to.to_string();
    }
    let scheme = from.scheme_str().unwrap_or("https");
    let authority = from.authority().map(|a| a.as_str()).unwrap_or("");
    let path = from.path();
    if let Some(host_path) = to.strip_prefix("//") {
        return format!("{scheme}://{host_path}");
    }
    if to.starts_with('/') {
        return format!("{scheme}://{authority}{to}");
    }
    if to.starts_with('?') {
        return format!("{scheme}://{authority}{path}{to}");
    }
    let dir = &path[..path.rfind('/').map_or(0, |i| i + 1)];
    format!("{scheme}://{authority}{dir}{to}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pinned root is "SCEI DNAS Root 05" (its SHA-256).
    #[test]
    fn sony_root_is_pinned() {
        let hex: String = ring::digest::digest(&ring::digest::SHA256, SONY_ROOT)
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            hex,
            "51d5a7833f67f1d1c6212f75997e4b83dada0f7910ba4b5a3fa4e9b7b9bc827c"
        );
    }

    /// Against the live server: run by hand (`--ignored`), since tests stay offline.
    #[test]
    #[ignore]
    fn sony_server_verifies() {
        let url = "https://sgst.prod.dl.playstation.net/sgst/prod/00/np/PPSA22946_00/6b99802e-852e-48d1-a4a0-01e3074d5f45-version.xml";
        let body = get(url, 1 << 20, &AtomicBool::new(false), &mut |_, _| {}).unwrap();
        assert!(body.starts_with(b"<?xml"));
    }
}
