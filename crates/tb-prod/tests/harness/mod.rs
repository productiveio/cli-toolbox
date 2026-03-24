#![allow(dead_code)]
use std::path::PathBuf;

use reqwest_middleware::ClientBuilder;
use rvcr::{VCRMiddleware, VCRMode};
use tb_prod::api::ProductiveClient;
use tb_prod::generic_cache::GenericCache;

pub fn cassette_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("cassettes")
        .join(format!("{}.json", name))
}

fn vcr_mode() -> VCRMode {
    match std::env::var("VCR_MODE").as_deref() {
        Ok("record") => VCRMode::Record,
        _ => VCRMode::Replay,
    }
}

fn load_credentials() -> (String, String) {
    // Try env vars first
    if let (Ok(token), Ok(org)) = (std::env::var("TB_PROD_TOKEN"), std::env::var("TB_PROD_ORG")) {
        return (token, org);
    }

    // Try secrets.toml at workspace root (CARGO_MANIFEST_DIR is crates/tb-prod,
    // but secrets.toml lives at the workspace root two levels up)
    let workspace_secrets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../secrets.toml");
    if workspace_secrets.exists() {
        if let Ok(content) = std::fs::read_to_string(&workspace_secrets) {
            if let Ok(table) = content.parse::<toml::Table>() {
                if let Some(section) = table.get("productive").and_then(|v| v.as_table()) {
                    let token = section.get("token").and_then(|v| v.as_str()).unwrap_or("");
                    let org = section
                        .get("org_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("10");
                    if !token.is_empty() {
                        return (token.to_string(), org.to_string());
                    }
                }
            }
        }
    }

    // Fall back to Config::load() (standalone config)
    if let Ok(config) = tb_prod::config::Config::load() {
        return (config.token, config.org_id);
    }

    // In replay mode, credentials don't matter (requests are replayed from cassettes)
    ("scrubbed".to_string(), "10".to_string())
}

pub fn test_client(cassette_name: &str) -> ProductiveClient {
    let path = cassette_path(cassette_name);
    let middleware = VCRMiddleware::try_from(path)
        .unwrap()
        .with_mode(vcr_mode())
        .with_modify_request(|req| {
            req.headers.remove("x-auth-token");
            req.headers.remove("x-organization-id");
        });

    let client = ClientBuilder::new(reqwest::Client::new())
        .with(middleware)
        .build();

    let (token, org_id) = load_credentials();

    ProductiveClient::with_client(client, &token, &org_id, "https://api.productive.io/api/v2")
}

pub fn test_cache() -> (tempfile::TempDir, GenericCache) {
    let tmp = tempfile::tempdir().unwrap();
    let cache = GenericCache::with_dir(tmp.path().to_path_buf()).unwrap();
    (tmp, cache)
}
