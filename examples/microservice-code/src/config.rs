//! Layered application configuration.
//!
//! Settings are resolved from environment variables. A variable that is
//! *absent* falls back to a sane default, but a variable that is present and
//! *malformed* is a startup error: the service refuses to boot rather than
//! silently running with a value you did not ask for. This mirrors the
//! `dotenv` + `convict`/`zod`-validated `process.env` pattern common in Node
//! services, but with parsing and validation centralised in one typed struct.

use std::env;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::time::Duration;

/// Error returned when an environment variable is present but invalid.
///
/// Surfacing this at startup (instead of falling back to a default) is the
/// "fail fast" rule from Section 28: a mistyped `PORT=eighty` should be a
/// loud boot failure, not a service quietly listening on 8080.
#[derive(Debug, thiserror::Error)]
#[error("invalid value for {key}: {value:?} ({reason})")]
pub struct ConfigError {
    /// Name of the offending environment variable.
    pub key: &'static str,
    /// The raw value found in the environment.
    pub value: String,
    /// Why the value was rejected.
    pub reason: String,
}

/// Strongly-typed application settings.
///
/// Construct with [`Settings::from_env`]. Every field has a default, so a
/// service started with an *empty* environment still boots — but any variable
/// that is set must parse, or startup fails with a [`ConfigError`].
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
    /// Log output format: `json` for structured logs, `pretty` for development.
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
    /// Build [`Settings`] from environment variables.
    ///
    /// Absent variables get the defaults below; present-but-invalid variables
    /// fail loudly.
    ///
    /// Recognised variables:
    /// - `HOST` (default `0.0.0.0`; must be an IP address)
    /// - `PORT` (default `8080`)
    /// - `BASE_URL` (default derived from host + port)
    /// - `CODE_LENGTH` (default `7`; must be 4..=32)
    /// - `REQUEST_TIMEOUT_SECS` (default `15`)
    /// - `LOG_FORMAT` (`json` | `pretty`, default `json`)
    /// - `RUST_LOG` (default `info,url_shortener=debug,tower_http=info`)
    pub fn from_env() -> Result<Self, ConfigError> {
        let host = env_var("HOST").unwrap_or_else(|| "0.0.0.0".to_string());
        let port = parse_var("PORT", 8080u16)?;

        let ip: IpAddr = host.parse().map_err(|_| ConfigError {
            key: "HOST",
            value: host.clone(),
            reason: "not a valid IP address".to_string(),
        })?;
        let bind_addr = SocketAddr::new(ip, port);

        // A 0.0.0.0 bind is not a usable link host, so advertise localhost.
        let advertised_host = if host == "0.0.0.0" {
            "localhost"
        } else {
            &host
        };
        let base_url =
            env_var("BASE_URL").unwrap_or_else(|| format!("http://{advertised_host}:{port}"));

        let code_length = parse_var("CODE_LENGTH", 7usize)?;
        if !(4..=32).contains(&code_length) {
            return Err(ConfigError {
                key: "CODE_LENGTH",
                value: code_length.to_string(),
                reason: "must be between 4 and 32".to_string(),
            });
        }

        let request_timeout = Duration::from_secs(parse_var("REQUEST_TIMEOUT_SECS", 15u64)?);

        let log_format = match env_var("LOG_FORMAT").as_deref() {
            None | Some("json") => LogFormat::Json,
            Some("pretty") => LogFormat::Pretty,
            Some(other) => {
                return Err(ConfigError {
                    key: "LOG_FORMAT",
                    value: other.to_string(),
                    reason: "expected \"json\" or \"pretty\"".to_string(),
                });
            }
        };

        let log_filter = env_var("RUST_LOG")
            .unwrap_or_else(|| "info,url_shortener=debug,tower_http=info".to_string());

        Ok(Settings {
            bind_addr,
            base_url,
            code_length,
            request_timeout,
            log_format,
            log_filter,
        })
    }
}

/// Read an environment variable, treating empty strings as absent.
fn env_var(key: &str) -> Option<String> {
    match env::var(key) {
        Ok(v) if !v.trim().is_empty() => Some(v),
        _ => None,
    }
}

/// Parse an environment variable into `T`.
///
/// Absent → `default`. Present but unparseable → [`ConfigError`] (fail fast).
fn parse_var<T>(key: &'static str, default: T) -> Result<T, ConfigError>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    match env_var(key) {
        None => Ok(default),
        Some(raw) => raw.parse().map_err(|e: T::Err| ConfigError {
            key,
            value: raw.clone(),
            reason: e.to_string(),
        }),
    }
}
