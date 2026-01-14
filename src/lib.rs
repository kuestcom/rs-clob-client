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
pub(crate) mod serde_helpers;
pub mod types;
#[cfg(any(feature = "ws", feature = "rtds"))]
pub mod ws;

use std::fmt::Write as _;

use alloy::primitives::ChainId;
use alloy::primitives::{B256, b256, keccak256};
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
        exchange: address!("0xE79717fE8456C620cFde6156b6AeAd79C4875Ca2"),
        collateral: address!("0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174"),
        conditional_tokens: address!("0x9432978d0f8A0E1a5317DD545B4a9ad32da8AD59"),
        neg_risk_adapter: None,
    },
    80002_u64 => ContractConfig {
        exchange: address!("0xE79717fE8456C620cFde6156b6AeAd79C4875Ca2"),
        collateral: address!("0x29604FdE966E3AEe42d9b5451BD9912863b3B904"),
        conditional_tokens: address!("0x9432978d0f8A0E1a5317DD545B4a9ad32da8AD59"),
        neg_risk_adapter: None,
    },
};

static NEG_RISK_CONFIG: phf::Map<ChainId, ContractConfig> = phf_map! {
    137_u64 => ContractConfig {
        exchange: address!("0xccBe425A0Aa24DCEf81f2e6edE3568a1683e7cbe"),
        collateral: address!("0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174"),
        conditional_tokens: address!("0x9432978d0f8A0E1a5317DD545B4a9ad32da8AD59"),
        neg_risk_adapter: Some(address!("0xc26DACF369DC1eA12421B9104031Cb5a2F8C9215")),
    },
    80002_u64 => ContractConfig {
        exchange: address!("0xccBe425A0Aa24DCEf81f2e6edE3568a1683e7cbe"),
        collateral: address!("0x29604FdE966E3AEe42d9b5451BD9912863b3B904"),
        conditional_tokens: address!("0x9432978d0f8A0E1a5317DD545B4a9ad32da8AD59"),
        neg_risk_adapter: Some(address!("0xc26DACF369DC1eA12421B9104031Cb5a2F8C9215")),
    },
};

// Wallet contract configurations for CREATE2 address derivation
static WALLET_CONFIG: phf::Map<ChainId, WalletContractConfig> = phf_map! {
    137_u64 => WalletContractConfig {
        proxy_factory: Some(address!("0xFe30Ff32E8fcB617E4665c5c94749ECc0808A6C9")),
        safe_factory: address!("0xA28927f4a23F52d0b7253c5E3d09a1fDb22977C4"),
    },
    80002_u64 => WalletContractConfig {
        proxy_factory: Some(address!("0xFe30Ff32E8fcB617E4665c5c94749ECc0808A6C9")),
        safe_factory: address!("0xA28927f4a23F52d0b7253c5E3d09a1fDb22977C4"),
    },
};

/// Init code hash for Kuest proxy wallet clones.
const PROXY_INIT_CODE_HASH: B256 =
    b256!("0x1f566e4d6fc92316ca3a8303965679f5ca265da52fec11f520dfd90ee773226f");

/// Init code hash for Gnosis Safe wallets
const SAFE_INIT_CODE_HASH: B256 =
    b256!("0x61e47bf36784271f639db33bb53fdc7fc843765357f63277759b9bb2ffdadaff");

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
}

/// Wallet contract configuration for CREATE2 address derivation
#[non_exhaustive]
#[derive(Debug)]
pub struct WalletContractConfig {
    /// Factory contract for Kuest proxy wallets (Magic/email wallets).
    pub proxy_factory: Option<Address>,
    /// Factory contract for Gnosis Safe wallets.
    pub safe_factory: Address,
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

/// Returns the wallet contract configuration for the given chain ID.
#[must_use]
pub fn wallet_contract_config(chain_id: ChainId) -> Option<&'static WalletContractConfig> {
    WALLET_CONFIG.get(&chain_id)
}

/// Derives the Kuest proxy wallet address for an EOA using CREATE2.
///
/// This is the deterministic address of the proxy wallet clone
/// that Kuest deploys for Magic/email wallet users.
///
/// # Arguments
/// * `eoa_address` - The externally owned account (EOA) address
/// * `chain_id` - The chain ID (e.g., 137 for Polygon mainnet)
///
/// # Returns
/// * `Some(Address)` - The derived proxy wallet address
/// * `None` - If the chain doesn't support proxy wallets or config is missing
#[must_use]
pub fn derive_proxy_wallet(eoa_address: Address, chain_id: ChainId) -> Option<Address> {
    let config = wallet_contract_config(chain_id)?;
    let factory = config.proxy_factory?;

    // Salt is keccak256(encodePacked(address)) - address is 20 bytes, no padding
    let salt = keccak256(eoa_address);

    Some(factory.create2(salt, PROXY_INIT_CODE_HASH))
}

/// Derives the Gnosis Safe wallet address for an EOA using CREATE2.
///
/// This is the deterministic address of the 1-of-1 Gnosis Safe multisig
/// that Kuest deploys for browser wallet users.
///
/// # Arguments
/// * `eoa_address` - The externally owned account (EOA) address
/// * `chain_id` - The chain ID (e.g., 137 for Polygon mainnet)
///
/// # Returns
/// * `Some(Address)` - The derived Safe wallet address
/// * `None` - If the chain config is missing
#[must_use]
pub fn derive_safe_wallet(eoa_address: Address, chain_id: ChainId) -> Option<Address> {
    let config = wallet_contract_config(chain_id)?;
    let factory = config.safe_factory;

    // Salt is keccak256(encodeAbiParameters(address)) - address padded to 32 bytes
    // ABI encoding pads address to 32 bytes (left-padded with zeros)
    let mut padded = [0_u8; 32];
    padded[12..].copy_from_slice(eoa_address.as_slice());
    let salt = keccak256(padded);

    Some(factory.create2(salt, SAFE_INIT_CODE_HASH))
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
                let _ = &e;
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
            address!("0xE79717fE8456C620cFde6156b6AeAd79C4875Ca2")
        );
    }

    #[test]
    fn config_contains_80002_neg() {
        let cfg = contract_config(AMOY, true).expect("missing config");
        assert_eq!(
            cfg.exchange,
            address!("0xccBe425A0Aa24DCEf81f2e6edE3568a1683e7cbe")
        );
    }

    #[test]
    fn wallet_contract_config_polygon() {
        let cfg = wallet_contract_config(POLYGON).expect("missing config");
        assert_eq!(
            cfg.proxy_factory,
            Some(address!("0xFe30Ff32E8fcB617E4665c5c94749ECc0808A6C9"))
        );
        assert_eq!(
            cfg.safe_factory,
            address!("0xA28927f4a23F52d0b7253c5E3d09a1fDb22977C4")
        );
    }

    #[test]
    fn wallet_contract_config_amoy() {
        let cfg = wallet_contract_config(AMOY).expect("missing config");
        assert_eq!(
            cfg.proxy_factory,
            Some(address!("0xFe30Ff32E8fcB617E4665c5c94749ECc0808A6C9"))
        );
        assert_eq!(
            cfg.safe_factory,
            address!("0xA28927f4a23F52d0b7253c5E3d09a1fDb22977C4")
        );
    }

    #[test]
    fn derive_safe_wallet_polygon() {
        // Test address: 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266 (Foundry/Anvil test key)
        let eoa = address!("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
        let safe_addr = derive_safe_wallet(eoa, POLYGON).expect("derivation failed");

        // This is the deterministic Safe address for this EOA on Polygon
        assert_eq!(
            safe_addr,
            address!("0x4a43509c513cd037d203f8cce37b3ee6c4473f39")
        );
    }

    #[test]
    fn derive_proxy_wallet_polygon() {
        // Test address: 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266 (Foundry/Anvil test key)
        let eoa = address!("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
        let proxy_addr = derive_proxy_wallet(eoa, POLYGON).expect("derivation failed");

        // This is the deterministic Proxy address for this EOA on Polygon
        assert_eq!(
            proxy_addr,
            address!("0x34c9fc98c31094271ecc6ba403d7476a8c131064")
        );
    }

    #[test]
    fn derive_proxy_wallet_amoy() {
        let eoa = address!("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
        let proxy_addr = derive_proxy_wallet(eoa, AMOY).expect("derivation failed");

        assert_eq!(
            proxy_addr,
            address!("0x34c9fc98c31094271ecc6ba403d7476a8c131064")
        );
    }

    #[test]
    fn derive_safe_wallet_amoy() {
        let eoa = address!("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
        // Safe wallet derivation should work on Amoy
        let safe_addr = derive_safe_wallet(eoa, AMOY).expect("derivation failed");

        // Same Safe factory on both networks, so same derived address
        assert_eq!(
            safe_addr,
            address!("0x4a43509c513cd037d203f8cce37b3ee6c4473f39")
        );
    }

    #[test]
    fn derive_wallet_unsupported_chain() {
        let eoa = address!("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
        // Unsupported chain should return None
        assert!(derive_proxy_wallet(eoa, 1).is_none());
        assert!(derive_safe_wallet(eoa, 1).is_none());
    }
}
