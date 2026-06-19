#![cfg_attr(doc, doc = include_str!("../README.md"))]

pub mod auth;
#[cfg(feature = "bridge")]
pub mod bridge;
#[cfg(feature = "clob")]
pub mod clob;
#[cfg(feature = "ctf")]
pub mod ctf;
#[cfg(feature = "data")]
pub mod data;
pub mod error;
#[cfg(feature = "gamma")]
pub mod gamma;
#[cfg(feature = "rtds")]
pub mod rtds;
#[cfg(any(feature = "clob", feature = "gamma"))]
pub(crate) mod sdk_site_config;
pub(crate) mod serde_helpers;
#[cfg(feature = "clob")]
pub(crate) mod site_scope;
pub mod types;
#[cfg(any(feature = "ws", feature = "rtds"))]
pub mod ws;

use std::fmt::Write as _;

use alloy::primitives::ChainId;
use phf::phf_map;
#[cfg(any(
    feature = "bridge",
    feature = "clob",
    feature = "data",
    feature = "gamma"
))]
use reqwest::{Request, StatusCode, header::HeaderMap};
use serde::Serialize;
#[cfg(any(
    feature = "bridge",
    feature = "clob",
    feature = "data",
    feature = "gamma"
))]
use serde::de::DeserializeOwned;

use crate::error::Error;
use crate::types::{Address, address};

pub type Result<T> = std::result::Result<T, Error>;

/// [`ChainId`] for Polygon mainnet
pub const POLYGON: ChainId = 137;

/// [`ChainId`] for Polygon testnet <https://polygon.technology/blog/introducing-the-amoy-testnet-for-polygon-pos>
pub const AMOY: ChainId = 80002;

pub const PRIVATE_KEY_VAR: &str = "KUEST_PRIVATE_KEY";

/// Timestamp in seconds since [`std::time::UNIX_EPOCH`]
pub(crate) type Timestamp = i64;

static CONFIG: phf::Map<ChainId, ContractConfig> = phf_map! {
    137_u64 => ContractConfig {
        exchange: address!("0xaCd95F4F42322c7bE215C170362EEc57Ef4E78c2"),
        collateral: address!("0x3c499c542cef5e3811e1192ce70d8cc03d5c3359"),
        conditional_tokens: address!("0x4682048725865bf17067bd85fF518527A262A9C7"),
        neg_risk_adapter: None,
        exchange_v2: Some(address!("0xaCd95F4F42322c7bE215C170362EEc57Ef4E78c2")),
    },
    80002_u64 => ContractConfig {
        exchange: address!("0xaCd95F4F42322c7bE215C170362EEc57Ef4E78c2"),
        collateral: address!("0x41E94Eb019C0762f9Bfcf9Fb1E58725BfB0e7582"),
        conditional_tokens: address!("0x4682048725865bf17067bd85fF518527A262A9C7"),
        neg_risk_adapter: None,
        exchange_v2: Some(address!("0xaCd95F4F42322c7bE215C170362EEc57Ef4E78c2")),
    },
};

static NEG_RISK_CONFIG: phf::Map<ChainId, ContractConfig> = phf_map! {
    137_u64 => ContractConfig {
        exchange: address!("0x961d3230B3BBdb2592D20fa34dBD12Fa19240603"),
        collateral: address!("0x3c499c542cef5e3811e1192ce70d8cc03d5c3359"),
        conditional_tokens: address!("0x4682048725865bf17067bd85fF518527A262A9C7"),
        neg_risk_adapter: Some(address!("0xd9416E904e1ab925ad72F03F6D6ce0Aa80fd2dC5")),
        exchange_v2: Some(address!("0x961d3230B3BBdb2592D20fa34dBD12Fa19240603")),
    },
    80002_u64 => ContractConfig {
        exchange: address!("0x961d3230B3BBdb2592D20fa34dBD12Fa19240603"),
        collateral: address!("0x41E94Eb019C0762f9Bfcf9Fb1E58725BfB0e7582"),
        conditional_tokens: address!("0x4682048725865bf17067bd85fF518527A262A9C7"),
        neg_risk_adapter: Some(address!("0xd9416E904e1ab925ad72F03F6D6ce0Aa80fd2dC5")),
        exchange_v2: Some(address!("0x961d3230B3BBdb2592D20fa34dBD12Fa19240603")),
    },
};

/// Helper struct to group the relevant deployed contract addresses
#[non_exhaustive]
#[derive(Debug)]
pub struct ContractConfig {
    pub exchange: Address,
    pub collateral: Address,
    pub conditional_tokens: Address,
    /// The Neg Risk Adapter contract address. Only present for neg-risk market configs.
    /// Users must approve this contract for token transfers to trade in neg-risk markets.
    pub neg_risk_adapter: Option<Address>,
    /// The V2 exchange contract address.
    pub exchange_v2: Option<Address>,
}

/// Given a `chain_id` and `is_neg_risk`, return the relevant [`ContractConfig`]
#[must_use]
pub fn contract_config(chain_id: ChainId, is_neg_risk: bool) -> Option<&'static ContractConfig> {
    if is_neg_risk {
        NEG_RISK_CONFIG.get(&chain_id)
    } else {
        CONFIG.get(&chain_id)
    }
}

/// Trait for converting request types to URL query parameters.
///
/// This trait is automatically implemented for all types that implement [`Serialize`].
/// It uses [`serde_html_form`] to serialize the struct fields into a query string.
/// Arrays are serialized as repeated keys (`key=val1&key=val2`).
pub trait ToQueryParams: Serialize {
    /// Converts the request to a URL query string.
    ///
    /// Returns an empty string if no parameters are set, otherwise returns
    /// a string starting with `?` followed by URL-encoded key-value pairs.
    /// Also uses an optional cursor as a parameter, if provided.
    fn query_params(&self, next_cursor: Option<&str>) -> String {
        let mut params = serde_html_form::to_string(self)
            .inspect_err(|e| {
                #[cfg(feature = "tracing")]
                tracing::error!("Unable to convert to URL-encoded string {e:?}");
                #[cfg(not(feature = "tracing"))]
                let _: &serde_html_form::ser::Error = e;
            })
            .unwrap_or_default();

        if let Some(cursor) = next_cursor {
            if !params.is_empty() {
                params.push('&');
            }
            let _ = write!(params, "next_cursor={cursor}");
        }

        if params.is_empty() {
            String::new()
        } else {
            format!("?{params}")
        }
    }
}

impl<T: Serialize> ToQueryParams for T {}

#[cfg(any(
    feature = "bridge",
    feature = "clob",
    feature = "data",
    feature = "gamma"
))]
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "debug",
        skip(client, request, headers),
        fields(
            method = %request.method(),
            path = request.url().path(),
            status_code
        )
    )
)]
async fn request<Response: DeserializeOwned>(
    client: &reqwest::Client,
    mut request: Request,
    headers: Option<HeaderMap>,
) -> Result<Response> {
    let method = request.method().clone();
    let path = request.url().path().to_owned();

    if let Some(h) = headers {
        *request.headers_mut() = h;
    }

    let response = client.execute(request).await?;
    let status_code = response.status();

    #[cfg(feature = "tracing")]
    tracing::Span::current().record("status_code", status_code.as_u16());

    if !status_code.is_success() {
        let message = response.text().await.unwrap_or_default();

        #[cfg(feature = "tracing")]
        tracing::warn!(
            status = %status_code,
            method = %method,
            path = %path,
            message = %message,
            "API request failed"
        );

        return Err(Error::status(status_code, method, path, message));
    }

    let json_value = response.json::<serde_json::Value>().await?;
    let response_data: Option<Response> = serde_helpers::deserialize_with_warnings(json_value)?;

    if let Some(response) = response_data {
        Ok(response)
    } else {
        #[cfg(feature = "tracing")]
        tracing::warn!(method = %method, path = %path, "API resource not found");
        Err(Error::status(
            StatusCode::NOT_FOUND,
            method,
            path,
            "Unable to find requested resource",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_contains_80002() {
        let cfg = contract_config(AMOY, false).expect("missing config");
        assert_eq!(
            cfg.exchange,
            address!("0xaCd95F4F42322c7bE215C170362EEc57Ef4E78c2")
        );
    }

    #[test]
    fn config_contains_80002_neg() {
        let cfg = contract_config(AMOY, true).expect("missing config");
        assert_eq!(
            cfg.exchange,
            address!("0x961d3230B3BBdb2592D20fa34dBD12Fa19240603")
        );
    }
}
