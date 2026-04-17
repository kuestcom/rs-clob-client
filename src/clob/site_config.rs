#[derive(Clone, Copy, Debug)]
pub struct SiteConfig {
    pub site_url: &'static str,
    pub fee_bps: u32,
    pub fee_receiver: &'static str,
    pub builder_mode: bool,
    pub geoblock: bool,
}

pub const SITE_CONFIG: SiteConfig = SiteConfig {
    site_url: "",
    fee_bps: 0,
    fee_receiver: "",
    builder_mode: false,
    geoblock: false,
};

pub const GEOBLOCK_HOST: &str = "https://geoblock.kuest.com";

pub fn order_fee_config() -> Option<(u32, &'static str)> {
    let fee_receiver = SITE_CONFIG.fee_receiver.trim();
    if fee_receiver.is_empty() {
        return None;
    }

    Some((SITE_CONFIG.fee_bps, fee_receiver))
}
