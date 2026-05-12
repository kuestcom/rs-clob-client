# Kuest Rust CLOB Client

Rust SDK for the Kuest CLOB.

## Install

```toml
[dependencies]
kuest-client-sdk = "0.5.1"
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

use alloy::signers::Signer as _;
use alloy::signers::local::LocalSigner;
use kuest_client_sdk::AMOY;
use kuest_client_sdk::clob::{Client, Config};
use kuest_client_sdk::clob::types::SignatureType;
use kuest_client_sdk::types::Address;

# async fn run() -> anyhow::Result<()> {
let signer = LocalSigner::from_str("<owner-private-key>")?.with_chain_id(Some(AMOY));
let deposit_wallet = Address::from_str("<deposit-wallet-address>")?;

let client = Client::new("https://clob.kuest.com", Config::default())?
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
