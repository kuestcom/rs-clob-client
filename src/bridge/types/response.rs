use alloy::primitives::U256;
use bon::Builder;
use serde::Deserialize;
use serde_with::{DisplayFromStr, serde_as};

use crate::types::{Address, ChainId, Decimal};

/// Response containing deposit addresses for different blockchain networks.
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize, PartialEq, Builder)]
pub struct DepositResponse {
    /// Deposit addresses for different blockchain networks.
    pub address: DepositAddresses,
    /// Additional information about supported chains.
    pub note: Option<String>,
}

/// Deposit addresses for different blockchain networks.
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize, PartialEq, Builder)]
#[builder(on(String, into))]
pub struct DepositAddresses {
    /// EVM-compatible deposit address (Ethereum, Polygon, Arbitrum, Base, etc.).
    pub evm: Address,
    /// Solana Virtual Machine deposit address.
    pub svm: String,
    /// Bitcoin deposit address.
    pub btc: String,
}

/// Response containing all supported assets for deposits.
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize, PartialEq, Builder)]
#[serde(rename_all = "camelCase")]
pub struct SupportedAssetsResponse {
    /// List of supported assets with minimum deposit amounts.
    pub supported_assets: Vec<SupportedAsset>,
    /// Additional information about supported chains and assets.
    pub note: Option<String>,
}

/// A supported asset with chain and token information.
#[non_exhaustive]
#[serde_as]
#[derive(Debug, Clone, Deserialize, PartialEq, Builder)]
#[builder(on(String, into))]
#[serde(rename_all = "camelCase")]
pub struct SupportedAsset {
    /// Blockchain chain ID (e.g., 1 for Ethereum mainnet, 137 for Polygon).
    /// Deserialized from JSON string representation (e.g., `"137"`).
    #[serde_as(as = "DisplayFromStr")]
    pub chain_id: ChainId,
    /// Human-readable chain name.
    pub chain_name: String,
    /// Token information.
    pub token: Token,
    /// Minimum deposit amount in USD.
    pub min_checkout_usd: Decimal,
}

/// Token information for a supported asset.
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize, PartialEq, Builder)]
#[builder(on(String, into))]
pub struct Token {
    /// Full token name.
    pub name: String,
    /// Token symbol.
    pub symbol: String,
    /// Token contract address.
    pub address: String,
    /// Token decimals.
    pub decimals: u8,
}

/// Transaction status for all deposits associated with a given deposit address.
#[non_exhaustive]
#[serde_as]
#[derive(Debug, Clone, Deserialize, PartialEq, Builder)]
#[builder(on(String, into))]
#[serde(rename_all = "camelCase")]
pub struct StatusResponse {
    /// List of transactions for the given address
    pub transactions: Vec<DepositTransaction>,
}

#[non_exhaustive]
#[serde_as]
#[derive(Debug, Clone, Deserialize, PartialEq, Builder)]
#[builder(on(String, into))]
#[serde(rename_all = "camelCase")]
pub struct DepositTransaction {
    /// Source chain ID
    #[serde_as(as = "DisplayFromStr")]
    pub from_chain_id: ChainId,
    /// Source token contract address
    pub from_token_address: String,
    /// Amount in base units (without decimals)
    #[serde_as(as = "DisplayFromStr")]
    pub from_amount_base_unit: U256,
    /// Destination chain ID
    #[serde_as(as = "DisplayFromStr")]
    pub to_chain_id: ChainId,
    /// Destination chain ID
    pub to_token_address: Address,
    /// Current status of the transaction
    pub status: DepositTransactionStatus,
    /// Transaction hash (only available when status is Completed)
    pub tx_hash: Option<String>,
    /// Unix timestamp in milliseconds when transaction was created (missing when status is `DepositDetected`)
    pub created_time_ms: Option<u64>,
}

#[non_exhaustive]
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DepositTransactionStatus {
    DepositDetected,
    Processing,
    OriginTxConfirmed,
    Submitted,
    Completed,
    Failed,
}
