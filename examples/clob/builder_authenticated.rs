//! Demonstrates builder API authentication with the CLOB client.
//!
//! This example shows how to:
//! 1. Authenticate as a regular user
//! 2. Create builder API credentials
//! 3. Promote the client to a builder client
//! 4. Access builder-specific endpoints
//!
//! Run with tracing enabled:
//! ```sh
//! RUST_LOG=info,hyper_util=off,hyper=off,reqwest=off,h2=off,rustls=off cargo run --example builder_authenticated --features clob,tracing
//! ```
//!
//! Optionally log to a file:
//! ```sh
//! LOG_FILE=builder_authenticated.log RUST_LOG=info,hyper_util=off,hyper=off,reqwest=off,h2=off,rustls=off cargo run --example builder_authenticated --features clob,tracing
//! ```
//!
//! Requires `KUEST_PRIVATE_KEY` and `KUEST_DEPOSIT_WALLET` environment variables to be set.

use std::fs::File;
use std::str::FromStr as _;

use alloy::signers::Signer as _;
use alloy::signers::local::LocalSigner;
use kuest_client_sdk::clob::types::request::TradesRequest;
use kuest_client_sdk::clob::{Client, Config};
use kuest_client_sdk::types::{B256, U256};
use kuest_client_sdk::{POLYGON, PRIVATE_KEY_VAR};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if let Ok(path) = std::env::var("LOG_FILE") {
        let file = File::create(path)?;
        tracing_subscriber::registry()
            .with(EnvFilter::from_default_env())
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(file)
                    .with_ansi(false),
            )
            .init();
    } else {
        tracing_subscriber::fmt::init();
    }

    let private_key = std::env::var(PRIVATE_KEY_VAR).expect("Need KUEST_PRIVATE_KEY");
    let signer = LocalSigner::from_str(&private_key)?.with_chain_id(Some(POLYGON));
    let funder = std::env::var("KUEST_DEPOSIT_WALLET")
        .expect("Need KUEST_DEPOSIT_WALLET")
        .parse()?;

    let client = Client::new("https://clob.kuest.com", Config::default())?
        .authentication_builder(&signer)
        .funder(funder)
        .authenticate()
        .await?;

    // Create builder credentials and promote to builder client
    let _builder_credentials = client.create_builder_api_key().await?;
    info!(
        endpoint = "create_builder_api_key",
        "created builder credentials"
    );

    let mut builder_code_bytes = [0_u8; 32];
    builder_code_bytes[12..].copy_from_slice(signer.address().as_slice());
    let builder_code = B256::from(builder_code_bytes);

    match client.builder_api_keys().await {
        Ok(keys) => info!(endpoint = "builder_api_keys", count = keys.len()),
        Err(e) => error!(endpoint = "builder_api_keys", error = %e),
    }

    let token_id = U256::from_str(
        "15871154585880608648532107628464183779895785213830018178010423617714102767076",
    )?;
    let request = TradesRequest::builder().asset_id(token_id).build();

    match client.builder_trades(builder_code, &request, None).await {
        Ok(trades) => {
            info!(endpoint = "builder_trades", builder_code = %builder_code, token_id = %token_id, count = trades.data.len());
        }
        Err(e) => {
            error!(endpoint = "builder_trades", builder_code = %builder_code, token_id = %token_id, error = %e);
        }
    }

    Ok(())
}
