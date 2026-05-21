<h1 align="center">
  <img src="https://github.com/user-attachments/assets/0cc687fb-89c4-43fa-a056-d89c307215ad" alt="Kuest" height="96" /><br/>
  Kuest Rust CLOB Client
</h1>

[![Crates.io](https://img.shields.io/crates/v/kuest-client-sdk.svg)](https://crates.io/crates/kuest-client-sdk)
[![CI](https://github.com/kuestcom/rs-clob-client/actions/workflows/ci.yml/badge.svg)](https://github.com/kuestcom/rs-clob-client/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/kuestcom/rs-clob-client/graph/badge.svg?token=FW1BYWWFJ2)](https://codecov.io/gh/kuestcom/rs-clob-client)

Rust SDK for the Kuest CLOB.

## Install

```toml
[dependencies]
kuest-client-sdk = "2.0.1"
```

## Read-Only Client

```rust,ignore
use kuest_client_sdk::clob::{Client, Config};

# async fn run() -> kuest_client_sdk::Result<()> {
let client = Client::new("https://clob.kuest.com", Config::default())?;
let ok = client.ok().await?;
println!("{ok}");
# Ok(())
# }
```

## Wallet-Only Trading

Kuest trading uses Deposit Wallet orders only. Public order builders use `SignatureType::DepositWallet` (`3`) and require a Deposit Wallet funder address.

```rust,ignore
use std::str::FromStr;

use alloy::primitives::B256;
use alloy::signers::Signer as _;
use alloy::signers::local::LocalSigner;
use kuest_client_sdk::POLYGON;
use kuest_client_sdk::clob::{Client, Config};
use kuest_client_sdk::clob::types::SignatureType;
use kuest_client_sdk::types::Address;

# async fn run() -> anyhow::Result<()> {
let signer = LocalSigner::from_str("<owner-private-key>")?.with_chain_id(Some(POLYGON));
let deposit_wallet = Address::from_str("<deposit-wallet-address>")?;
let site_builder_code = std::env::var("SITE_BUILDER_CODE").ok();

let mut config = Config::builder().use_server_time(true);
if let Some(builder_code) = site_builder_code.filter(|value| !value.trim().is_empty()) {
    config = config.builder_code(B256::from_str(&builder_code)?);
}

let client = Client::new("https://clob.kuest.com", config.build())?
    .authentication_builder(&signer)
    .signature_type(SignatureType::DepositWallet)
    .funder(deposit_wallet)
    .authenticate()
    .await?;
# Ok(())
# }
```

## Notes

- USDC remains the settlement collateral.
- Auth headers sent to Kuest services remain `KUEST_*`.
- Neutral local aliases such as `PRIVATE_KEY` or `API_KEY` can be used by applications before constructing SDK config.
