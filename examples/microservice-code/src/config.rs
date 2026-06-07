//! Layered application configuration.
//!
//! Settings are resolved from environment variables, falling back to sane
//! defaults when a variable is absent or cannot be parsed. This mirrors the
//! `dotenv` + `convict`/`zod`-validated `process.env` pattern common in Node
//! services, but with parsing and validation centralised in one typed struct.

use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

/// Strongly-typed application settings.
///
/// Construct with [`Settings::from_env`], which never panics: every field has a
/// default, so a service started with an empty environment still boots.
#[derive(Debug, Clone)]
pub struct Settings {
    /// Address the HTTP server binds to (host + port).
    pub bind_addr: SocketAddr,
    /// Public base URL used when building short links (e.g. `http://localhost:8080`).
    pub base_url: String,
    /// Length of the generated short code (number of base62 characters).
    pub code_length: usize,
    /// Per-request timeout applied by the Tower timeout layer.
    pub request_timeout: Duration,
    /// Log output format: `json` for structured logs, anything else for pretty.
    pub log_format: LogFormat,
    /// Logging filter directive (the value normally found in `RUST_LOG`).
    pub log_filter: String,
}

/// Selects the `tracing-subscriber` output formatter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// Machine-readable JSON, one object per line — ideal for log aggregators.
    Json,
    /// Human-readable, colourised output for local development.
    Pretty,
}

impl Settings {
    /// Build [`Settings`] from environment variables with defaults applied.
    ///
    /// Recognised variables:
    /// - `HOST` (default `0.0.0.0`)
    /// - `PORT` (default `8080`)
    /// - `BASE_URL` (default derived from host + port)
    /// - `CODE_LENGTH` (default `7`)
    /// - `REQUEST_TIMEOUT_SECS` (default `15`)
    /// - `LOG_FORMAT` (`json` | `pretty`, default `json`)
    /// - `RUST_LOG` (default `info,url_shortener=debug,tower_http=info`)
    pub fn from_env() -> Self {
        let host = env_var("HOST").unwrap_or_else(|| "0.0.0.0".to_string());
        let port = parse_or("PORT", 8080u16);

        let ip: IpAddr = host
            .parse()
            .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        let bind_addr = SocketAddr::new(ip, port);

        // A 0.0.0.0 bind is not a usable link host, so advertise localhost.
        let advertised_host = if host == "0.0.0.0" { "localhost" } else { &host };
        let base_url =
            env_var("BASE_URL").unwrap_or_else(|| format!("http://{advertised_host}:{port}"));

        let code_length = parse_or("CODE_LENGTH", 7usize).clamp(4, 32);
        let request_timeout = Duration::from_secs(parse_or("REQUEST_TIMEOUT_SECS", 15u64));

        let log_format = match env_var("LOG_FORMAT").as_deref() {
            Some("pretty") => LogFormat::Pretty,
            _ => LogFormat::Json,
        };

        let log_filter = env_var("RUST_LOG")
            .unwrap_or_else(|| "info,url_shortener=debug,tower_http=info".to_string());

        Settings {
            bind_addr,
            base_url,
            code_length,
            request_timeout,
            log_format,
            log_filter,
        }
    }
}

/// Read an environment variable, treating empty strings as absent.
fn env_var(key: &str) -> Option<String> {
    match env::var(key) {
        Ok(v) if !v.trim().is_empty() => Some(v),
        _ => None,
    }
}

/// Parse an environment variable into `T`, falling back to `default` on any error.
fn parse_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    env_var(key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
