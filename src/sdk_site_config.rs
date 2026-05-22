use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::Deserialize;

const ZERO_BYTES32: &str = "0x0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(crate) struct SiteConfig {
    pub site_url: String,
    pub builder_mode: bool,
    pub geoblock: bool,
    pub builder_code: String,
    pub order_metadata: String,
}

impl Default for SiteConfig {
    fn default() -> Self {
        Self {
            site_url: String::new(),
            builder_mode: false,
            geoblock: false,
            builder_code: String::new(),
            order_metadata: ZERO_BYTES32.to_string(),
        }
    }
}

static SITE_CONFIG: OnceLock<SiteConfig> = OnceLock::new();

fn site_config_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".sdk/site-config.json")
}

fn load_site_config() -> SiteConfig {
    let path = site_config_path();
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == ErrorKind::NotFound => return SiteConfig::default(),
        Err(error) => panic!("failed to read {}: {error}", path.display()),
    };

    serde_json::from_str(&contents)
        .unwrap_or_else(|error| panic!("invalid {}: {error}", path.display()))
}

pub(crate) fn site_config() -> &'static SiteConfig {
    SITE_CONFIG.get_or_init(load_site_config)
}

pub(crate) fn site_url() -> Option<String> {
    let value = site_config().site_url.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

pub(crate) fn builder_code() -> Option<String> {
    let value = site_config().builder_code.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

pub(crate) fn order_metadata() -> Option<String> {
    let value = site_config().order_metadata.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}
