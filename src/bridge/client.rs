use reqwest::{
    Client as ReqwestClient, Method,
    header::{HeaderMap, HeaderValue},
};
use url::Url;

use super::types::{DepositRequest, DepositResponse, SupportedAssetsResponse};
use crate::error::Error;
use crate::Result;

const DEFAULT_HOST: &str = "https://bridge.kuest.com/#disabled";
const DISABLED_HOST: &str = "bridge.kuest.com";

fn is_disabled_host(host: &Url) -> bool {
    host.fragment().is_some() || host.host_str() == Some(DISABLED_HOST)
}

/// Client for the Kuest Bridge API.
///
/// The Bridge API enables bridging assets from various chains (EVM, Solana, Bitcoin)
/// to USDC.e on Polygon for trading on Kuest.
///
/// # Example
///
/// ```no_run
/// use kuest_client_sdk::types::address;
/// use kuest_client_sdk::bridge::{Client, types::DepositRequest};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let client = Client::default();
///
/// // Get deposit addresses
/// let request = DepositRequest::builder()
///     .address(address!("56687bf447db6ffa42ffe2204a05edaa20f55839"))
///     .build();
/// let response = client.deposit(&request).await?;
///
/// // Get supported assets
/// let assets = client.supported_assets().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct Client {
    host: Url,
    client: ReqwestClient,
    disabled: bool,
}

impl Default for Client {
    fn default() -> Self {
        Client::new(DEFAULT_HOST)
            .expect("Client with default endpoint should succeed")
    }
}

impl Client {
    /// Creates a new Bridge API client with a custom host.
    ///
    /// # Errors
    ///
    /// Returns an error if the host URL is invalid or the HTTP client fails to build.
    pub fn new(host: &str) -> Result<Client> {
        let mut headers = HeaderMap::new();

        headers.insert("User-Agent", HeaderValue::from_static("rs_clob_client"));
        headers.insert("Accept", HeaderValue::from_static("*/*"));
        headers.insert("Connection", HeaderValue::from_static("keep-alive"));
        headers.insert("Content-Type", HeaderValue::from_static("application/json"));
        let client = ReqwestClient::builder().default_headers(headers).build()?;
        let host = Url::parse(host)?;
        let disabled = is_disabled_host(&host);

        Ok(Self {
            host,
            client,
            disabled,
        })
    }

    /// Returns the host URL for the client.
    #[must_use]
    pub fn host(&self) -> &Url {
        &self.host
    }

    #[must_use]
    fn client(&self) -> &ReqwestClient {
        &self.client
    }

    fn ensure_enabled(&self) -> Result<()> {
        if self.disabled {
            return Err(Error::validation("Bridge desativada"));
        }
        Ok(())
    }

    /// Create deposit addresses for a Kuest wallet.
    ///
    /// Generates unique deposit addresses for bridging assets to Kuest.
    /// Returns addresses for EVM-compatible chains, Solana, and Bitcoin.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use kuest_client_sdk::types::address;
    /// use kuest_client_sdk::bridge::{Client, types::DepositRequest};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = Client::default();
    /// let request = DepositRequest::builder()
    ///     .address(address!("56687bf447db6ffa42ffe2204a05edaa20f55839"))
    ///     .build();
    ///
    /// let response = client.deposit(&request).await?;
    /// println!("EVM: {}", response.address.evm);
    /// println!("SVM: {}", response.address.svm);
    /// println!("BTC: {}", response.address.btc);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn deposit(&self, request: &DepositRequest) -> Result<DepositResponse> {
        self.ensure_enabled()?;
        let request = self
            .client()
            .request(Method::POST, format!("{}deposit", self.host()))
            .json(request)
            .build()?;

        crate::request(&self.client, request, None).await
    }

    /// Get all supported chains and tokens for deposits.
    ///
    /// Returns information about which assets can be deposited and their
    /// minimum deposit amounts in USD.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use kuest_client_sdk::bridge::Client;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = Client::default();
    /// let response = client.supported_assets().await?;
    ///
    /// for asset in response.supported_assets {
    ///     println!(
    ///         "{} ({}) on {} - min: ${:.2}",
    ///         asset.token.name,
    ///         asset.token.symbol,
    ///         asset.chain_name,
    ///         asset.min_checkout_usd
    ///     );
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn supported_assets(&self) -> Result<SupportedAssetsResponse> {
        self.ensure_enabled()?;
        let request = self
            .client()
            .request(Method::GET, format!("{}supported-assets", self.host()))
            .build()?;

        crate::request(&self.client, request, None).await
    }
}
