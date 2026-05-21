use std::str::FromStr as _;

use crate::types::B256;

pub(crate) const SITE_BUILDER_CODE: &str = "";
pub(crate) const SITE_ORDER_METADATA: &str =
    "0x0000000000000000000000000000000000000000000000000000000000000000";

pub(crate) fn builder_code() -> Option<B256> {
    let value = SITE_BUILDER_CODE.trim();
    if value.is_empty() {
        return None;
    }

    let parsed = B256::from_str(value).ok()?;
    if parsed == B256::ZERO {
        return None;
    }

    Some(parsed)
}

pub(crate) fn order_metadata() -> Option<B256> {
    let value = SITE_ORDER_METADATA.trim();
    if value.is_empty() {
        return None;
    }

    let parsed = B256::from_str(value).ok()?;
    if parsed == B256::ZERO {
        return None;
    }

    Some(parsed)
}
