#![cfg(feature = "clob")]
#![allow(
    clippy::unwrap_used,
    reason = "Do not need additional syntax for setting up tests, and https://github.com/rust-lang/rust-clippy/issues/13981"
)]
#![allow(
    unused,
    reason = "Deeply nested uses in sub-modules are falsely flagged as being unused"
)]

use std::str::FromStr as _;

use alloy::primitives::U256;
use alloy::signers::Signer as _;
use alloy::signers::k256::ecdsa::SigningKey;
use alloy::signers::local::LocalSigner;
use httpmock::MockServer;
use kuest_client_sdk::POLYGON;
use kuest_client_sdk::auth::Normal;
use kuest_client_sdk::auth::state::Authenticated;
use kuest_client_sdk::clob::types::{SignatureType, TickSize};
use kuest_client_sdk::clob::{Client, Config};
use kuest_client_sdk::types::Decimal;
use reqwest::StatusCode;
use serde_json::json;
use uuid::Uuid;

// publicly known private key
pub const PRIVATE_KEY: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
pub const PASSPHRASE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub const SECRET: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

pub const SIGNATURE: &str = "0xfdfb5abf512e439ea61c8595c18e527e718bf16010acf57cef51d09e15893098275d3c6f73038f36ec0cd0ce55436fca14dc64b11611f4dce896e354207508cc1b";
pub const TIMESTAMP: &str = "100000";

pub const BUILDER_PASSPHRASE: &str = "passphrase";
pub const TOKEN_1: &str = "1";

pub const KUEST_ADDRESS: &str = "KUEST_ADDRESS";
pub const KUEST_API_KEY: &str = "KUEST_API_KEY";
pub const KUEST_NONCE: &str = "KUEST_NONCE";
pub const KUEST_PASSPHRASE: &str = "KUEST_PASSPHRASE";
pub const KUEST_SIGNATURE: &str = "KUEST_SIGNATURE";
pub const KUEST_TIMESTAMP: &str = "KUEST_TIMESTAMP";

pub const KUEST_BUILDER_API_KEY: &str = "KUEST_BUILDER_API_KEY";
pub const KUEST_BUILDER_PASSPHRASE: &str = "KUEST_BUILDER_PASSPHRASE";
pub const KUEST_BUILDER_SIGNATURE: &str = "KUEST_BUILDER_SIGNATURE";
pub const KUEST_BUILDER_TIMESTAMP: &str = "KUEST_BUILDER_TIMESTAMP";

pub const API_KEY: Uuid = Uuid::nil();
pub const BUILDER_API_KEY: Uuid = Uuid::max();

pub const USDC_DECIMALS: u32 = 6;

pub type TestClient = Client<Authenticated<Normal>>;

pub async fn create_authenticated(server: &MockServer) -> anyhow::Result<TestClient> {
    let signer = LocalSigner::from_str(PRIVATE_KEY)?.with_chain_id(Some(POLYGON));

    let mock = server.mock(|when, then| {
        when.method(httpmock::Method::GET)
            .path("/auth/derive-api-key")
            .header(KUEST_ADDRESS, signer.address().to_string().to_lowercase())
            .header(KUEST_NONCE, "0")
            .header(KUEST_SIGNATURE, SIGNATURE)
            .header(KUEST_TIMESTAMP, TIMESTAMP);
        then.status(StatusCode::OK).json_body(json!({
            "apiKey": API_KEY.to_string(),
            "passphrase": PASSPHRASE,
            "secret": SECRET
        }));
    });
    let mock2 = server.mock(|when, then| {
        when.method(httpmock::Method::GET).path("/time");
        then.status(StatusCode::OK)
            .json_body(TIMESTAMP.parse::<i64>().unwrap());
    });

    let config = Config::builder().use_server_time(true).build();
    let client = Client::new(&server.base_url(), config)?
        .authentication_builder(&signer)
        .authenticate()
        .await?;

    mock.assert();
    mock2.assert_calls(2);

    Ok(client)
}

pub fn ensure_requirements(server: &MockServer, token_id: &str, tick_size: TickSize) {
    server.mock(|when, then| {
        when.method(httpmock::Method::GET).path("/neg-risk");
        then.status(StatusCode::OK)
            .json_body(json!({ "neg_risk": false }));
    });

    server.mock(|when, then| {
        when.method(httpmock::Method::GET).path("/fee-rate");
        then.status(StatusCode::OK)
            .json_body(json!({ "base_fee": 0 }));
    });

    server.mock(|when, then| {
        when.method(httpmock::Method::GET)
            .path("/tick-size")
            .query_param("token_id", token_id);
        then.status(StatusCode::OK).json_body(json!({
                "minimum_tick_size": tick_size.as_decimal(),
        }));
    });
}

pub fn to_decimal(value: U256) -> Decimal {
    Decimal::from_str_exact(&value.to_string()).unwrap()
}
