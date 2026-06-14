use std::collections::BTreeSet;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use reqwest::{Client as ReqwestClient, Method};
use serde_json::Value;
use url::Url;

use crate::Result;
use crate::error::Error;
use crate::sdk_site_config;
use crate::types::{B256, U256};

const SITE_SCOPE_TTL: Duration = Duration::from_secs(5 * 60);
const SITE_EVENTS_LIMIT: usize = 100;
const SITE_EVENTS_MAX_PAGES: usize = 50;

#[derive(Clone, Debug, Default)]
pub(crate) struct SiteMarketScope {
    condition_ids: BTreeSet<String>,
    token_ids: BTreeSet<String>,
}

impl SiteMarketScope {
    pub(crate) fn is_empty(&self) -> bool {
        self.condition_ids.is_empty() && self.token_ids.is_empty()
    }
}

#[derive(Clone, Debug)]
struct SiteScopeCacheEntry {
    scope: SiteMarketScope,
    expires_at: Instant,
}

static SITE_SCOPE_CACHE: OnceLock<Mutex<Option<SiteScopeCacheEntry>>> = OnceLock::new();

fn cache() -> &'static Mutex<Option<SiteScopeCacheEntry>> {
    SITE_SCOPE_CACHE.get_or_init(|| Mutex::new(None))
}

pub(crate) fn has_configured_site_scope() -> bool {
    sdk_site_config::site_url().is_some()
}

fn normalize_site_origin(raw: &str) -> Result<Url> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(Error::validation(
            "site_url must be configured for site-scoped market discovery",
        ));
    }

    let candidate = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_owned()
    } else {
        format!("https://{trimmed}")
    };

    let mut url = Url::parse(&candidate)?;
    url.set_path("");
    url.set_query(None);
    url.set_fragment(None);

    Ok(url)
}

fn normalize_condition_id(value: &str) -> Option<String> {
    value
        .trim()
        .parse::<B256>()
        .ok()
        .map(|condition_id| condition_id.to_string().to_ascii_lowercase())
}

fn normalize_token_id(value: &str) -> Option<String> {
    value
        .trim()
        .parse::<U256>()
        .ok()
        .map(|token_id| token_id.to_string())
}

fn value_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn add_condition_id(scope: &mut SiteMarketScope, value: Option<&Value>) {
    let Some(value) = value.and_then(value_string) else {
        return;
    };
    if let Some(condition_id) = normalize_condition_id(&value) {
        scope.condition_ids.insert(condition_id);
    }
}

fn add_token_id(scope: &mut SiteMarketScope, value: Option<&Value>) {
    let Some(value) = value.and_then(value_string) else {
        return;
    };
    if let Some(token_id) = normalize_token_id(&value) {
        scope.token_ids.insert(token_id);
    }
}

fn collect_market_scope(value: &Value, scope: &mut SiteMarketScope) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_market_scope(item, scope);
            }
        }
        Value::Object(object) => {
            add_condition_id(scope, object.get("condition_id"));
            add_condition_id(scope, object.get("conditionId"));
            add_condition_id(scope, object.get("conditionID"));
            add_condition_id(scope, object.get("c"));

            add_token_id(scope, object.get("token_id"));
            add_token_id(scope, object.get("tokenId"));
            add_token_id(scope, object.get("asset_id"));
            add_token_id(scope, object.get("assetId"));
            add_token_id(scope, object.get("t"));

            for key in [
                "markets",
                "outcomes",
                "tokens",
                "clob_token_ids",
                "clobTokenIds",
                "outcome_assets",
                "outcomeAssets",
            ] {
                if let Some(child) = object.get(key) {
                    collect_market_scope(child, scope);
                }
            }
        }
        _ => {}
    }
}

async fn fetch_site_market_scope(client: &ReqwestClient) -> Result<SiteMarketScope> {
    let Some(site_url) = sdk_site_config::site_url() else {
        return Ok(SiteMarketScope::default());
    };

    let origin = normalize_site_origin(&site_url)?;
    let mut scope = SiteMarketScope::default();
    for page in 0..SITE_EVENTS_MAX_PAGES {
        let mut url = origin.join("api/events")?;
        url.query_pairs_mut()
            .append_pair("status", "active")
            .append_pair("includeBookmarkState", "false")
            .append_pair("limit", &SITE_EVENTS_LIMIT.to_string())
            .append_pair("offset", &(page * SITE_EVENTS_LIMIT).to_string());

        let request = client.request(Method::GET, url.clone()).build()?;
        let response = client.execute(request).await?;
        let status_code = response.status();
        if !status_code.is_success() {
            let message = response.text().await.unwrap_or_default();
            return Err(Error::status(
                status_code,
                Method::GET,
                url.path().to_owned(),
                message,
            ));
        }

        let payload = response.json::<Value>().await?;
        let Some(events) = payload.as_array() else {
            return Err(Error::validation(
                "site-scoped market discovery expected /api/events to return an array",
            ));
        };

        collect_market_scope(&payload, &mut scope);
        if events.len() < SITE_EVENTS_LIMIT {
            break;
        }
    }

    Ok(scope)
}

pub(crate) async fn get_site_market_scope(client: &ReqwestClient) -> Result<SiteMarketScope> {
    if !has_configured_site_scope() {
        return Ok(SiteMarketScope::default());
    }

    {
        let cache = cache()
            .lock()
            .map_err(|error| Error::validation(format!("site scope cache poisoned: {error}")))?;
        if let Some(entry) = cache.as_ref()
            && entry.expires_at > Instant::now()
        {
            return Ok(entry.scope.clone());
        }
    }

    let scope = fetch_site_market_scope(client).await?;
    let mut cache = cache()
        .lock()
        .map_err(|error| Error::validation(format!("site scope cache poisoned: {error}")))?;
    *cache = Some(SiteScopeCacheEntry {
        scope: scope.clone(),
        expires_at: Instant::now() + SITE_SCOPE_TTL,
    });
    Ok(scope)
}

pub(crate) fn condition_allowed(scope: &SiteMarketScope, condition_id: &B256) -> bool {
    scope
        .condition_ids
        .contains(&condition_id.to_string().to_ascii_lowercase())
}

pub(crate) fn token_allowed(scope: &SiteMarketScope, token_id: &U256) -> bool {
    scope.token_ids.contains(&token_id.to_string())
}
