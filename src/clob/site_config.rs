use std::str::FromStr as _;

use crate::sdk_site_config;
use crate::types::B256;

fn parse_b256(value: Option<String>) -> Option<B256> {
    let value = value?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let parsed = B256::from_str(trimmed)
        .unwrap_or_else(|error| panic!("invalid bytes32 value in .sdk/site-config.json: {error}"));
    if parsed == B256::ZERO {
        return None;
    }

    Some(parsed)
}

pub(crate) fn builder_code() -> Option<B256> {
    parse_b256(sdk_site_config::builder_code())
}

pub(crate) fn order_metadata() -> Option<B256> {
    parse_b256(sdk_site_config::order_metadata())
}
