#![cfg(feature = "bridge")]
#![allow(clippy::unwrap_used, reason = "tests can panic on unwrap")]

mod deposit {
    use httpmock::{Method::POST, MockServer};
    use kuest_client_sdk::bridge::{
        Client,
        types::{DepositAddresses, DepositRequest, DepositResponse},
    };
    use kuest_client_sdk::types::address;
    use reqwest::StatusCode;
    use serde_json::json;

    #[tokio::test]
    async fn deposit_should_succeed() -> anyhow::Result<()> {
        let server = MockServer::start();
        let client = Client::new(&server.base_url())?;

        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/deposit")
                .header("Content-Type", "application/json")
                .json_body(json!({
                    "address": "0x56687bf447db6ffa42ffe2204a05edaa20f55839"
                }));
            then.status(StatusCode::CREATED).json_body(json!({
                "address": {
                    "evm": "0x23566f8b2E82aDfCf01846E54899d110e97AC053",
                    "svm": "CrvTBvzryYxBHbWu2TiQpcqD5M7Le7iBKzVmEj3f36Jb",
                    "btc": "bc1q8eau83qffxcj8ht4hsjdza3lha9r3egfqysj3g"
                },
                "note": "Only certain chains and tokens are supported. See /supported-assets for details."
            }));
        });

        let request = DepositRequest::builder()
            .address(address!("56687bf447db6ffa42ffe2204a05edaa20f55839"))
            .build();

        let response = client.deposit(&request).await?;

        let expected = DepositResponse::builder()
            .address(
                DepositAddresses::builder()
                    .evm(address!("23566f8b2E82aDfCf01846E54899d110e97AC053"))
                    .svm("CrvTBvzryYxBHbWu2TiQpcqD5M7Le7iBKzVmEj3f36Jb")
                    .btc("bc1q8eau83qffxcj8ht4hsjdza3lha9r3egfqysj3g")
                    .build(),
            )
            .note(
                "Only certain chains and tokens are supported. See /supported-assets for details."
                    .to_owned(),
            )
            .build();

        assert_eq!(response, expected);
        mock.assert();

        Ok(())
    }

    #[tokio::test]
    async fn deposit_without_note_should_succeed() -> anyhow::Result<()> {
        let server = MockServer::start();
        let client = Client::new(&server.base_url())?;

        let mock = server.mock(|when, then| {
            when.method(POST).path("/deposit");
            then.status(StatusCode::CREATED).json_body(json!({
                "address": {
                    "evm": "0x23566f8b2E82aDfCf01846E54899d110e97AC053",
                    "svm": "CrvTBvzryYxBHbWu2TiQpcqD5M7Le7iBKzVmEj3f36Jb",
                    "btc": "bc1q8eau83qffxcj8ht4hsjdza3lha9r3egfqysj3g"
                }
            }));
        });

        let request = DepositRequest::builder()
            .address(address!("56687bf447db6ffa42ffe2204a05edaa20f55839"))
            .build();

        let response = client.deposit(&request).await?;

        assert!(response.note.is_none());
        assert_eq!(
            response.address.evm,
            address!("23566f8b2E82aDfCf01846E54899d110e97AC053")
        );
        mock.assert();

        Ok(())
    }

    #[tokio::test]
    async fn deposit_bad_request_should_fail() -> anyhow::Result<()> {
        let server = MockServer::start();
        let client = Client::new(&server.base_url())?;

        let mock = server.mock(|when, then| {
            when.method(POST).path("/deposit");
            then.status(StatusCode::BAD_REQUEST)
                .json_body(json!({"error": "Invalid address"}));
        });

        let request = DepositRequest::builder()
            .address(address!("0000000000000000000000000000000000000000"))
            .build();

        let result = client.deposit(&request).await;

        result.unwrap_err();
        mock.assert();

        Ok(())
    }
}

mod supported_assets {
    use httpmock::{Method::GET, MockServer};
    use kuest_client_sdk::bridge::{
        Client,
        types::{SupportedAsset, SupportedAssetsResponse, Token},
    };
    use reqwest::StatusCode;
    use rust_decimal_macros::dec;
    use serde_json::json;

    #[tokio::test]
    async fn supported_assets_should_succeed() -> anyhow::Result<()> {
        let server = MockServer::start();
        let client = Client::new(&server.base_url())?;

        let mock = server.mock(|when, then| {
            when.method(GET).path("/supported-assets");
            then.status(StatusCode::OK).json_body(json!({
                "supportedAssets": [
                    {
                        "chainId": "1",
                        "chainName": "Ethereum",
                        "token": {
                            "name": "USD Coin",
                            "symbol": "USDC",
                            "address": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
                            "decimals": 6
                        },
                        "minCheckoutUsd": 45.0
                    },
                    {
                        "chainId": "137",
                        "chainName": "Polygon",
                        "token": {
                            "name": "Bridged USDC",
                            "symbol": "USDC.e",
                            "address": "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174",
                            "decimals": 6
                        },
                        "minCheckoutUsd": 10.0
                    }
                ]
            }));
        });

        let response = client.supported_assets().await?;

        let expected = SupportedAssetsResponse::builder()
            .supported_assets(vec![
                SupportedAsset::builder()
                    .chain_id(1_u64)
                    .chain_name("Ethereum")
                    .token(
                        Token::builder()
                            .name("USD Coin")
                            .symbol("USDC")
                            .address("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48")
                            .decimals(6_u8)
                            .build(),
                    )
                    .min_checkout_usd(dec!(45))
                    .build(),
                SupportedAsset::builder()
                    .chain_id(137_u64)
                    .chain_name("Polygon")
                    .token(
                        Token::builder()
                            .name("Bridged USDC")
                            .symbol("USDC.e")
                            .address("0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174")
                            .decimals(6_u8)
                            .build(),
                    )
                    .min_checkout_usd(dec!(10))
                    .build(),
            ])
            .build();

        assert_eq!(response, expected);
        mock.assert();

        Ok(())
    }

    #[tokio::test]
    async fn supported_assets_empty_should_succeed() -> anyhow::Result<()> {
        let server = MockServer::start();
        let client = Client::new(&server.base_url())?;

        let mock = server.mock(|when, then| {
            when.method(GET).path("/supported-assets");
            then.status(StatusCode::OK)
                .json_body(json!({"supportedAssets": []}));
        });

        let response = client.supported_assets().await?;

        assert!(response.supported_assets.is_empty());
        mock.assert();

        Ok(())
    }

    #[tokio::test]
    async fn supported_assets_server_error_should_fail() -> anyhow::Result<()> {
        let server = MockServer::start();
        let client = Client::new(&server.base_url())?;

        let mock = server.mock(|when, then| {
            when.method(GET).path("/supported-assets");
            then.status(StatusCode::INTERNAL_SERVER_ERROR)
                .json_body(json!({"error": "Internal server error"}));
        });

        let result = client.supported_assets().await;

        result.unwrap_err();
        mock.assert();

        Ok(())
    }
}

mod deposit_status {
    use alloy::primitives::{U256, address};
    use httpmock::{Method::GET, MockServer};
    use kuest_client_sdk::bridge::{
        Client,
        types::{DepositTransaction, DepositTransactionStatus, StatusRequest, StatusResponse},
    };
    use reqwest::StatusCode;
    use serde_json::json;

    #[tokio::test]
    async fn deposit_status_should_succeed() -> anyhow::Result<()> {
        let server = MockServer::start();
        let client = Client::new(&server.base_url())?;

        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/status/0x9cb12Ec30568ab763ae5891ce4b8c5C96CeD72C9");
            then.status(StatusCode::OK).json_body(json!({
                "transactions": [
                    {
                        "fromChainId": "1",
                        "fromTokenAddress": "11111111111111111111111111111111",
                        "fromAmountBaseUnit": "13566635",
                        "toChainId": "137",
                        "toTokenAddress": "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174",
                        "status": "COMPLETED",
                        "txHash": "3atr19NAiNCYt24RHM1WnzZp47RXskpTDzspJoCBBaMFwUB8fk37hFkxz35P5UEnnmWz21rb2t5wJ8pq3EE2XnxU",
                        "createdTimeMs": 1_757_646_914_535_u64,

                    },
                    {
                        "fromChainId": "2",
                        "fromTokenAddress": "11111111111111111111111111111111",
                        "fromAmountBaseUnit": "13_566_635",
                        "toChainId": "137",
                        "toTokenAddress": "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174",
                        "status": "DEPOSIT_DETECTED",

                    }
                ]
            }));
        });

        let request = StatusRequest::builder()
            .address("0x9cb12Ec30568ab763ae5891ce4b8c5C96CeD72C9")
            .build();
        let response = client.status(&request).await?;

        let expected = StatusResponse::builder()
            .transactions(vec![
                DepositTransaction::builder()
                    .from_chain_id(1)
                    .from_token_address("11111111111111111111111111111111")
                    .from_amount_base_unit(U256::from(13_566_635))
                    .to_chain_id(137)
                    .to_token_address(address!("0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174"))
                    .status(DepositTransactionStatus::Completed)
                    .tx_hash("3atr19NAiNCYt24RHM1WnzZp47RXskpTDzspJoCBBaMFwUB8fk37hFkxz35P5UEnnmWz21rb2t5wJ8pq3EE2XnxU")
                    .created_time_ms(1_757_646_914_535)
                    .build(),
                DepositTransaction::builder()
                    .from_chain_id(2)
                    .from_token_address("11111111111111111111111111111111")
                    .from_amount_base_unit(U256::from(13_566_635))
                    .to_chain_id(137)
                    .to_token_address(address!("0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174"))
                    .status(DepositTransactionStatus::DepositDetected)
                    .build(),
            ])
            .build();

        assert_eq!(response, expected);
        mock.assert();

        Ok(())
    }
}

mod client {
    use kuest_client_sdk::bridge::Client;

    #[test]
    fn default_client_should_have_correct_host() {
        let client = Client::default();
        assert_eq!(client.host().as_str(), "https://bridge.kuest.com/#disabled");
    }

    #[test]
    fn custom_host_should_succeed() -> anyhow::Result<()> {
        let client = Client::new("https://custom.bridge.api")?;
        assert_eq!(client.host().as_str(), "https://custom.bridge.api/");
        Ok(())
    }

    #[test]
    fn invalid_host_should_fail() {
        let result = Client::new("not a valid url");
        result.unwrap_err();
    }
}
