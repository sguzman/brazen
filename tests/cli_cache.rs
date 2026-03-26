use brazen::cli_cache::{CacheCliOptions, fetch_and_store};
use brazen::config::BrazenConfig;
use brazen::platform_paths::RuntimePaths;
use tempfile::tempdir;
use tiny_http::{Header, Response, Server};

#[test]
fn cache_cli_fetch_records_asset() {
    let server = Server::http("127.0.0.1:0").unwrap();
    let addr = server.server_addr().to_ip().expect("expected ip address");
    let url = format!("http://{addr}/example");

    std::thread::spawn(move || {
        if let Ok(request) = server.recv() {
            let header = Header::from_bytes("Content-Type", "text/plain; charset=utf-8").unwrap();
            let response = Response::from_string("brazen-cache").with_header(header);
            let _ = request.respond(response);
        }
    });

    let dir = tempdir().unwrap();
    let runtime = RuntimePaths {
        config_path: dir.path().join("brazen.toml"),
        data_dir: dir.path().join("data"),
        logs_dir: dir.path().join("logs"),
        profiles_dir: dir.path().join("profiles"),
        cache_dir: dir.path().join("cache"),
        downloads_dir: dir.path().join("downloads"),
        crash_dumps_dir: dir.path().join("crash"),
        active_profile_dir: dir.path().join("profiles/default"),
        session_path: dir.path().join("profiles/default/session.json"),
    };
    let config = BrazenConfig::default();

    let options = CacheCliOptions {
        url,
        profile: None,
        timeout_secs: 10,
        stats: true,
        insecure: false,
    };
    let result = fetch_and_store(&config, &runtime, &options).unwrap();
    assert_eq!(result.metadata.mime, "text/plain");
    assert_eq!(result.metadata.size_bytes, "brazen-cache".len() as u64);
    assert!(result.metadata.hash.is_some());
    assert_eq!(result.entry_count, 1);
}
