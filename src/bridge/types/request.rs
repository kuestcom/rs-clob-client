use bon::Builder;
use serde::Serialize;

use crate::types::Address;

/// Request to create deposit addresses for a Kuest wallet.
///
/// # Example
///
/// ```
/// use kuest_client_sdk::types::address;
/// use kuest_client_sdk::bridge::types::DepositRequest;
///
/// let request = DepositRequest::builder()
///     .address(address!("56687bf447db6ffa42ffe2204a05edaa20f55839"))
///     .build();
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Builder)]
pub struct DepositRequest {
    /// The Kuest wallet address to generate deposit addresses for.
    pub address: Address,
}

/// Request to get deposit statuses for a given deposit address.
///
/// ### Note: This doesn't use the alloy Address type, since it supports Solana and Bitcoin addresses.
///
/// # Example
///
/// ```
/// use kuest_client_sdk::bridge::types::StatusRequest;
///
/// let request = StatusRequest::builder().address("0x9cb12Ec30568ab763ae5891ce4b8c5C96CeD72C9").build();
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Builder)]
#[builder(on(String, into))]
pub struct StatusRequest {
    pub address: String,
}
