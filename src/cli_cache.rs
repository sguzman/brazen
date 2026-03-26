use std::collections::BTreeMap;
use std::io::Read;
use std::time::Duration;

use chrono::Utc;

use crate::cache::{AssetMetadata, AssetQuery, AssetStore, StorageMode};
use crate::config::BrazenConfig;
use crate::logging::init_tracing;
use crate::platform_paths::{PlatformPaths, RuntimePaths};
use crate::tls::install_crypto_provider;

#[derive(Debug, Clone)]
pub struct CacheCliOptions {
    pub profile: Option<String>,
    pub timeout_secs: u64,
    pub stats: bool,
    pub insecure: bool,
    pub command: CacheCliCommand,
}

#[derive(Debug, Clone)]
pub enum CacheCliCommand {
    Fetch {
        url: String,
    },
    List {
        filters: CacheCliFilters,
        limit: usize,
    },
    Show {
        key: String,
    },
    Export {
        path: String,
        summary: bool,
        jsonl: bool,
    },
    Import {
        path: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct CacheCliFilters {
    pub url: Option<String>,
    pub mime: Option<String>,
    pub hash: Option<String>,
    pub session_id: Option<String>,
    pub tab_id: Option<String>,
    pub status_code: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct CacheFetchResult {
    pub metadata: AssetMetadata,
    pub entry_count: usize,
    pub storage_mode: StorageMode,
}

pub fn parse_cache_args(args: &[String]) -> Result<CacheCliOptions, String> {
    if args.is_empty() || args[0] != "cache" {
        return Err("expected `cache <url>` or `cache <command>`".to_string());
    }
    let mut profile = None;
    let mut timeout_secs = 30u64;
    let mut stats = false;
    let mut insecure = false;
    let mut i = 1;
    let command_token = args
        .get(i)
        .ok_or_else(|| "missing cache command".to_string())?;
    let command = match command_token.as_str() {
        "list" | "--list" => {
            i += 1;
            let (filters, limit) = parse_list_args(args, &mut i, &mut profile)?;
            CacheCliCommand::List { filters, limit }
        }
        "show" | "--show" => {
            i += 1;
            let key = args
                .get(i)
                .ok_or_else(|| "missing asset id or hash".to_string())?
                .to_string();
            i += 1;
            parse_profile_flags(args, &mut i, &mut profile)?;
            CacheCliCommand::Show { key }
        }
        "export" => {
            i += 1;
            let path = args
                .get(i)
                .ok_or_else(|| "missing export path".to_string())?
                .to_string();
            i += 1;
            let (summary, jsonl) = parse_export_flags(args, &mut i, &mut profile)?;
            CacheCliCommand::Export {
                path,
                summary,
                jsonl,
            }
        }
        "import" => {
            i += 1;
            let path = args
                .get(i)
                .ok_or_else(|| "missing import path".to_string())?
                .to_string();
            i += 1;
            parse_profile_flags(args, &mut i, &mut profile)?;
            CacheCliCommand::Import { path }
        }
        value => {
            let url = value.to_string();
            i += 1;
            parse_fetch_flags(
                args,
                &mut i,
                &mut profile,
                &mut timeout_secs,
                &mut stats,
                &mut insecure,
            )?;
            CacheCliCommand::Fetch { url }
        }
    };

    Ok(CacheCliOptions {
        profile,
        timeout_secs,
        stats,
        insecure,
        command,
    })
}

fn parse_fetch_flags(
    args: &[String],
    i: &mut usize,
    profile: &mut Option<String>,
    timeout_secs: &mut u64,
    stats: &mut bool,
    insecure: &mut bool,
) -> Result<(), String> {
    while *i < args.len() {
        match args[*i].as_str() {
            "--profile" => {
                *i += 1;
                *profile = args.get(*i).cloned();
            }
            "--timeout" => {
                *i += 1;
                let value = args
                    .get(*i)
                    .ok_or_else(|| "missing timeout value".to_string())?;
                *timeout_secs = value
                    .parse::<u64>()
                    .map_err(|_| "timeout must be integer seconds".to_string())?;
            }
            "--stats" => {
                *stats = true;
            }
            "--insecure" => {
                *insecure = true;
            }
            value => {
                return Err(format!("unrecognized argument `{value}`"));
            }
        }
        *i += 1;
    }
    Ok(())
}

fn parse_list_args(
    args: &[String],
    i: &mut usize,
    profile: &mut Option<String>,
) -> Result<(CacheCliFilters, usize), String> {
    let mut filters = CacheCliFilters::default();
    let mut limit = 50usize;
    while *i < args.len() {
        match args[*i].as_str() {
            "--profile" => {
                *i += 1;
                *profile = args.get(*i).cloned();
            }
            "--url" => {
                *i += 1;
                filters.url = args.get(*i).cloned();
            }
            "--mime" => {
                *i += 1;
                filters.mime = args.get(*i).cloned();
            }
            "--hash" => {
                *i += 1;
                filters.hash = args.get(*i).cloned();
            }
            "--session" => {
                *i += 1;
                filters.session_id = args.get(*i).cloned();
            }
            "--tab" => {
                *i += 1;
                filters.tab_id = args.get(*i).cloned();
            }
            "--status" => {
                *i += 1;
                let value = args
                    .get(*i)
                    .ok_or_else(|| "missing status value".to_string())?;
                filters.status_code = Some(
                    value
                        .parse::<u16>()
                        .map_err(|_| "status must be a number".to_string())?,
                );
            }
            "--limit" => {
                *i += 1;
                let value = args
                    .get(*i)
                    .ok_or_else(|| "missing limit value".to_string())?;
                limit = value
                    .parse::<usize>()
                    .map_err(|_| "limit must be a number".to_string())?;
            }
            value => {
                return Err(format!("unrecognized argument `{value}`"));
            }
        }
        *i += 1;
    }
    Ok((filters, limit))
}

fn parse_export_flags(
    args: &[String],
    i: &mut usize,
    profile: &mut Option<String>,
) -> Result<(bool, bool), String> {
    let mut summary = false;
    let mut jsonl = true;
    while *i < args.len() {
        match args[*i].as_str() {
            "--profile" => {
                *i += 1;
                *profile = args.get(*i).cloned();
            }
            "--summary" => {
                summary = true;
            }
            "--json" => {
                jsonl = false;
            }
            "--jsonl" => {
                jsonl = true;
            }
            value => {
                return Err(format!("unrecognized argument `{value}`"));
            }
        }
        *i += 1;
    }
    Ok((summary, jsonl))
}

fn parse_profile_flags(
    args: &[String],
    i: &mut usize,
    profile: &mut Option<String>,
) -> Result<(), String> {
    while *i < args.len() {
        match args[*i].as_str() {
            "--profile" => {
                *i += 1;
                *profile = args.get(*i).cloned();
            }
            value => {
                return Err(format!("unrecognized argument `{value}`"));
            }
        }
        *i += 1;
    }
    Ok(())
}

pub fn run_cache_cli(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_cache_args(args).map_err(|error| format!("cache args error: {error}"))?;
    install_crypto_provider();
    let platform = PlatformPaths::detect()?;
    let config_path = platform.default_config_path();
    let mut config = BrazenConfig::load_with_defaults(&config_path)?;
    if let Some(profile) = options.profile.clone() {
        config.profiles.active_profile = profile;
    }
    let runtime = platform.resolve_runtime_paths(&config, &config_path)?;
    init_tracing(&config.logging, &runtime.logs_dir)?;

    match options.command.clone() {
        CacheCliCommand::Fetch { url } => {
            let result = fetch_and_store(&config, &runtime, &options, &url)?;
            println!(
                "cached id={} url={} status={} mime={} size={} hash={} body_key={} storage={:?} entries={}",
                result.metadata.asset_id,
                result.metadata.url,
                result
                    .metadata
                    .status_code
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                result.metadata.mime,
                result.metadata.size_bytes,
                result
                    .metadata
                    .hash
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
                result
                    .metadata
                    .body_key
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
                result.storage_mode,
                result.entry_count
            );
            if options.stats {
                println!(
                    "capture_mode={:?} truncated={} third_party={} authenticated={}",
                    result.metadata.capture_mode,
                    result.metadata.truncated,
                    result.metadata.is_third_party,
                    result.metadata.authenticated
                );
            }

            tracing::info!(
                target: "brazen::cache::cli",
                url = %result.metadata.url,
                mime = %result.metadata.mime,
                size_bytes = result.metadata.size_bytes,
                hash = ?result.metadata.hash,
                storage_mode = ?result.storage_mode,
                entries = result.entry_count,
                "cache cli fetch complete"
            );
        }
        CacheCliCommand::List { filters, limit } => {
            let store = AssetStore::load(
                config.cache.clone(),
                &runtime,
                config.profiles.active_profile.clone(),
            );
            let results = store.query(AssetQuery {
                url: filters.url,
                mime: filters.mime,
                hash: filters.hash,
                session_id: filters.session_id,
                tab_id: filters.tab_id,
                status_code: filters.status_code,
            });
            for entry in results.into_iter().take(limit) {
                println!(
                    "{} {} {} {} {}",
                    entry.created_at,
                    entry
                        .status_code
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    entry.mime,
                    entry.url,
                    entry.hash.unwrap_or_else(|| "-".to_string())
                );
            }
        }
        CacheCliCommand::Show { key } => {
            let store = AssetStore::load(
                config.cache.clone(),
                &runtime,
                config.profiles.active_profile.clone(),
            );
            let entry = store
                .find_by_id_or_hash(&key)
                .ok_or_else(|| format!("cache entry not found for id/hash `{key}`"))?;
            println!("{}", serde_json::to_string_pretty(entry)?);
        }
        CacheCliCommand::Export {
            path,
            summary,
            jsonl,
        } => {
            let store = AssetStore::load(
                config.cache.clone(),
                &runtime,
                config.profiles.active_profile.clone(),
            );
            let export_path = std::path::PathBuf::from(path);
            if jsonl {
                store.export_jsonl(&export_path)?;
            } else {
                store.export_json(&export_path)?;
            }
            if summary {
                let summary_path = export_path.with_extension("summary.txt");
                store.export_summary(&summary_path)?;
                println!("summary: {}", summary_path.display());
            }
            println!("exported: {}", export_path.display());
        }
        CacheCliCommand::Import { path } => {
            let mut store = AssetStore::load(
                config.cache.clone(),
                &runtime,
                config.profiles.active_profile.clone(),
            );
            let count = store.import_json_merge(std::path::PathBuf::from(path).as_path())?;
            println!("imported: {}", count);
        }
    }

    Ok(())
}

pub fn fetch_and_store(
    config: &BrazenConfig,
    runtime: &RuntimePaths,
    options: &CacheCliOptions,
    url: &str,
) -> Result<CacheFetchResult, Box<dyn std::error::Error>> {
    install_crypto_provider();
    let mut agent_builder = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(options.timeout_secs))
        .redirects(10);
    if options.insecure {
        let tls_config = build_insecure_tls_config()?;
        agent_builder = agent_builder.tls_config(std::sync::Arc::new(tls_config));
    }
    let agent = agent_builder.build();

    let started_at = Utc::now();
    let response = agent
        .get(url)
        .call()
        .map_err(|error| format!("cache fetch failed: {error}"))?;
    let status = response.status();
    if status >= 400 {
        return Err(format!("cache fetch returned status {status}").into());
    }

    let final_url = response.get_url().to_string();
    let mut headers = BTreeMap::new();
    for name in response.headers_names() {
        if let Some(value) = response.header(&name) {
            headers.insert(name.to_lowercase(), value.to_string());
        }
    }
    let mime = response
        .header("content-type")
        .and_then(|value| value.split(';').next())
        .unwrap_or("application/octet-stream")
        .trim()
        .to_string();

    let mut body = Vec::new();
    let mut reader = response.into_reader();
    reader.read_to_end(&mut body)?;
    let finished_at = Utc::now();

    let mut store = AssetStore::load(
        config.cache.clone(),
        runtime,
        config.profiles.active_profile.clone(),
    );

    let metadata = store.record_asset_with_timing(
        url,
        Some(final_url),
        Some("GET".to_string()),
        Some(status),
        &mime,
        Some(&body),
        headers,
        false,
        false,
        Some("cli".to_string()),
        None,
        None,
        Some(started_at.to_rfc3339()),
        Some(finished_at.to_rfc3339()),
    )?;

    Ok(CacheFetchResult {
        metadata,
        entry_count: store.entries().len(),
        storage_mode: store.storage_mode(),
    })
}

fn build_insecure_tls_config() -> Result<rustls::ClientConfig, Box<dyn std::error::Error>> {
    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};

    #[derive(Debug)]
    struct InsecureVerifier;

    impl ServerCertVerifier for InsecureVerifier {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp: &[u8],
            _now: UnixTime,
        ) -> Result<ServerCertVerified, rustls::Error> {
            Ok(ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &rustls::DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &rustls::DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            rustls::crypto::ring::default_provider()
                .signature_verification_algorithms
                .supported_schemes()
        }
    }

    let root_store = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let tls_config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let mut tls_config = tls_config;
    tls_config
        .dangerous()
        .set_certificate_verifier(std::sync::Arc::new(InsecureVerifier));
    Ok(tls_config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cache_args_accepts_url_and_flags() {
        let args = vec![
            "cache".to_string(),
            "https://example.com".to_string(),
            "--profile".to_string(),
            "dev".to_string(),
            "--timeout".to_string(),
            "12".to_string(),
            "--stats".to_string(),
            "--insecure".to_string(),
        ];
        let options = parse_cache_args(&args).unwrap();
        match options.command {
            CacheCliCommand::Fetch { url } => assert_eq!(url, "https://example.com"),
            _ => panic!("expected fetch command"),
        }
        assert_eq!(options.profile.as_deref(), Some("dev"));
        assert_eq!(options.timeout_secs, 12);
        assert!(options.stats);
        assert!(options.insecure);
    }

    #[test]
    fn parse_cache_args_accepts_list_show_export() {
        let args = vec![
            "cache".to_string(),
            "list".to_string(),
            "--url".to_string(),
            "example".to_string(),
            "--status".to_string(),
            "200".to_string(),
            "--limit".to_string(),
            "5".to_string(),
        ];
        let options = parse_cache_args(&args).unwrap();
        match options.command {
            CacheCliCommand::List { filters, limit } => {
                assert_eq!(filters.url.as_deref(), Some("example"));
                assert_eq!(filters.status_code, Some(200));
                assert_eq!(limit, 5);
            }
            _ => panic!("expected list command"),
        }

        let args = vec![
            "cache".to_string(),
            "show".to_string(),
            "asset-1".to_string(),
        ];
        let options = parse_cache_args(&args).unwrap();
        match options.command {
            CacheCliCommand::Show { key } => assert_eq!(key, "asset-1"),
            _ => panic!("expected show command"),
        }

        let args = vec![
            "cache".to_string(),
            "export".to_string(),
            "out.jsonl".to_string(),
            "--summary".to_string(),
        ];
        let options = parse_cache_args(&args).unwrap();
        match options.command {
            CacheCliCommand::Export {
                path,
                summary,
                jsonl,
            } => {
                assert_eq!(path, "out.jsonl");
                assert!(summary);
                assert!(jsonl);
            }
            _ => panic!("expected export command"),
        }
    }
}
