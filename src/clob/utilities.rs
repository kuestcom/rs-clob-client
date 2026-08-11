//! Client-side utility functions for orderbook analysis, fee calculation, and price validation.

use std::fmt::Write as _;

use rust_decimal::prelude::ToPrimitive as _;
use rust_decimal::{Decimal, RoundingStrategy};
use rust_decimal_macros::dec;
use sha1::Digest as _;

use super::types::response::{OrderBookSummaryResponse, OrderSummary};
use super::types::{Amount, AmountInner, OrderType, Side, TickSize};
use crate::Result;
use crate::error::Error;

/// Number of decimal places in a USDC amount on-chain. Exposed so utility callers can
/// use the same truncation semantics.
pub const USDC_DECIMALS: u32 = 6;
const FEE_DECIMALS: u32 = 5;

/// Exact single-fill fee components produced by the Kuest curve and Exchange split.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DynamicFeeBreakdown {
    pub kuest_fee_base: Decimal,
    pub kuest_fee: Decimal,
    pub operator_fee: Decimal,
    pub total_fee: Decimal,
}

/// Mirrors CLOB five-decimal rounding and the Exchange's integer-USDC operator split.
pub fn calculate_dynamic_fee_breakdown(
    shares: Decimal,
    price: Decimal,
    fee_rate: Decimal,
    fee_exponent: Decimal,
    builder_taker_fee_share_bps: u32,
) -> Result<DynamicFeeBreakdown> {
    if price <= Decimal::ZERO || price >= Decimal::ONE {
        return Err(Error::validation(format!(
            "price {price} must be between 0 and 1 for dynamic fee calculation"
        )));
    }
    if builder_taker_fee_share_bps >= 10_000 {
        return Err(Error::validation(
            "builder_taker_fee_share_bps must be between 0 and 9999",
        ));
    }
    let exponent = u32::try_from(fee_exponent).map_err(|_| {
        Error::validation(format!(
            "fee exponent {fee_exponent} must be a non-negative integer"
        ))
    })?;
    let raw_base = shares
        .checked_mul(fee_rate)
        .and_then(|value| {
            value.checked_mul(decimal_pow(price * (Decimal::ONE - price), exponent).ok()?)
        })
        .ok_or_else(|| Error::validation("dynamic fee calculation overflow"))?;
    if raw_base < dec!(0.00001) {
        return Ok(DynamicFeeBreakdown {
            kuest_fee_base: Decimal::ZERO,
            kuest_fee: Decimal::ZERO,
            operator_fee: Decimal::ZERO,
            total_fee: Decimal::ZERO,
        });
    }

    let kuest_fee_base =
        raw_base.round_dp_with_strategy(FEE_DECIMALS, RoundingStrategy::MidpointAwayFromZero);
    let total_fee = (kuest_fee_base * Decimal::from(10_000_u32)
        / Decimal::from(10_000_u32 - builder_taker_fee_share_bps))
    .round_dp_with_strategy(FEE_DECIMALS, RoundingStrategy::ToPositiveInfinity);
    let total_micro_usdc = (total_fee * Decimal::from(1_000_000_u32))
        .to_u64()
        .ok_or_else(|| Error::validation("total fee does not fit in USDC units"))?;
    let operator_micro_usdc = total_micro_usdc * u64::from(builder_taker_fee_share_bps) / 10_000;
    let usdc_scale = Decimal::from(1_000_000_u32);

    Ok(DynamicFeeBreakdown {
        kuest_fee_base,
        kuest_fee: Decimal::from(total_micro_usdc - operator_micro_usdc) / usdc_scale,
        operator_fee: Decimal::from(operator_micro_usdc) / usdc_scale,
        total_fee: Decimal::from(total_micro_usdc) / usdc_scale,
    })
}

/// Walks orderbook levels best-to-worst, accumulating via `accumulate`, and returns
/// the cutoff price where cumulative ≥ `target`.
///
/// The CLOB wire format delivers levels worst-first (asks descending, bids ascending),
/// so iterating with `.rev()` produces the natural matching order. This means
/// `levels[0]` is the worst price in the slice.
///
/// If no level satisfies the target:
/// - Returns `None` for [`OrderType::FOK`]
/// - Returns the worst price in the slice (`levels[0]`) for other order types, so a
///   market-order caller has a safe upper/lower bound for its limit price.
///
/// Returns `None` for empty `levels`.
pub(crate) fn walk_levels<F: Fn(&OrderSummary) -> Decimal>(
    levels: &[OrderSummary],
    target: Decimal,
    accumulate: F,
    order_type: &OrderType,
) -> Option<Decimal> {
    if levels.is_empty() {
        return None;
    }

    let mut total = Decimal::ZERO;
    for level in levels.iter().rev() {
        total += accumulate(level);
        if total >= target {
            return Some(level.price);
        }
    }

    if *order_type == OrderType::FOK {
        return None;
    }

    Some(levels[0].price)
}

/// Walks the orderbook to calculate the effective fill price for a given [`Amount`].
///
/// The unit of `amount` (USDC vs shares) determines which side of the book is walked
/// and how liquidity is accumulated:
///
/// | Side | Amount  | Walks | Accumulates      |
/// |------|---------|-------|------------------|
/// | Buy  | Usdc    | asks  | `size * price`   |
/// | Buy  | Shares  | asks  | `size`           |
/// | Sell | Shares  | bids  | `size`           |
/// | Sell | Usdc    | — invalid, returns a validation error     |
///
/// # Errors
/// - `Side::Sell` paired with an `Amount::usdc(_)` (SELL orders must size in shares).
/// - `Side::Unknown`.
/// - `OrderType::FOK` with insufficient liquidity at any level.
///
/// For non-FOK order types with insufficient liquidity, returns the worst price in the
/// walked side of the book (a safe upper/lower bound for a market-order limit price).
pub fn calculate_market_price(
    orderbook: &OrderBookSummaryResponse,
    side: Side,
    amount: Amount,
    order_type: &OrderType,
) -> Result<Decimal> {
    let (levels, acc): (&[OrderSummary], fn(&OrderSummary) -> Decimal) = match (side, amount.0) {
        (Side::Buy, AmountInner::Usdc(_)) => (&orderbook.asks, |l| l.size * l.price),
        (Side::Buy, AmountInner::Shares(_)) => (&orderbook.asks, |l| l.size),
        (Side::Sell, AmountInner::Shares(_)) => (&orderbook.bids, |l| l.size),
        (Side::Sell, AmountInner::Usdc(_)) => {
            return Err(Error::validation(
                "SELL orders must specify their amount in shares, not USDC",
            ));
        }
        (Side::Unknown, _) => return Err(Error::validation(format!("Invalid side: {side}"))),
    };

    walk_levels(levels, amount.as_inner(), acc, order_type).ok_or_else(|| {
        Error::validation(format!(
            "Insufficient liquidity to fill {} on {side:?}",
            amount.as_inner()
        ))
    })
}

/// Generates a server-compatible SHA1 hash of an orderbook snapshot.
///
/// Constructs a compact JSON payload with a specific key order
/// (`market`, `asset_id`, `timestamp`, `hash=""`, `bids`, `asks`,
/// `min_order_size`, `tick_size`, `neg_risk`, `last_trade_price`)
/// and returns the SHA1 hex digest.
///
/// **Note**: [`OrderBookSummaryResponse::hash()`] uses SHA-256 on `serde_json::to_string`
/// and produces different results. This function is for server-compatible verification.
#[must_use]
pub fn orderbook_summary_hash(orderbook: &OrderBookSummaryResponse) -> String {
    // Build JSON manually — serde_json::json! uses BTreeMap which sorts keys alphabetically,
    // but the server expects a specific non-alphabetical key order.
    let mut json = String::with_capacity(512);

    json.push('{');
    let _ = write!(json, "\"market\":\"{}\"", orderbook.market);

    let asset_id_json = serde_json::to_string(&orderbook.asset_id).unwrap_or_default();
    let _ = write!(json, ",\"asset_id\":{asset_id_json}");
    let _ = write!(
        json,
        ",\"timestamp\":\"{}\"",
        orderbook.timestamp.timestamp_millis()
    );
    json.push_str(",\"hash\":\"\"");

    json.push_str(",\"bids\":[");
    for (i, o) in orderbook.bids.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        let _ = write!(
            json,
            "{{\"price\":\"{}\",\"size\":\"{}\"}}",
            o.price, o.size
        );
    }
    json.push(']');

    json.push_str(",\"asks\":[");
    for (i, o) in orderbook.asks.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        let _ = write!(
            json,
            "{{\"price\":\"{}\",\"size\":\"{}\"}}",
            o.price, o.size
        );
    }
    json.push(']');

    let _ = write!(json, ",\"min_order_size\":\"{}\"", orderbook.min_order_size);
    let _ = write!(
        json,
        ",\"tick_size\":\"{}\"",
        Decimal::from(orderbook.tick_size)
    );
    let _ = write!(json, ",\"neg_risk\":{}", orderbook.neg_risk);
    let last = orderbook.last_trade_price.unwrap_or(Decimal::ZERO);
    let _ = write!(json, ",\"last_trade_price\":\"{last}\"");
    json.push('}');

    let mut hasher = sha1::Sha1::new();
    hasher.update(json.as_bytes());
    let result = hasher.finalize();

    alloy::hex::encode(result)
}

/// Adjusts a market-buy USDC amount for the grossed-up dynamic taker fee.
///
/// Returns `amount` unchanged when `user_usdc_balance` already covers the total cost.
/// Otherwise shrinks it so principal + fees = balance, then truncates to [`USDC_DECIMALS`]
/// (matching the on-chain USDC scale). Returned amount is ready to pass to
/// [`Amount::usdc`](super::types::Amount::usdc).
///
/// # Errors
/// - `user_usdc_balance` is below the minimum to cover one USDC-unit of fees; the adjusted
///   amount would truncate to zero, which would submit a zero-value order the backend
///   rejects with an opaque error. Callers should top up the balance and retry.
pub fn adjust_market_buy_amount(
    amount: Decimal,
    user_usdc_balance: Decimal,
    price: Decimal,
    fee_rate: Decimal,
    fee_exponent: Decimal,
    builder_taker_fee_share: Decimal,
) -> Result<Decimal> {
    if price <= Decimal::ZERO || price >= Decimal::ONE {
        return Err(Error::validation(format!(
            "price {price} must be between 0 and 1 for dynamic fee calculation"
        )));
    }
    let share_bps_decimal = builder_taker_fee_share * Decimal::from(10_000_u32);
    let builder_share_bps = share_bps_decimal
        .to_u32()
        .ok_or_else(|| Error::validation("builder taker fee share must be between 0 and 0.9999"))?;
    if Decimal::from(builder_share_bps) != share_bps_decimal || builder_share_bps >= 10_000 {
        return Err(Error::validation(
            "builder taker fee share must have whole-bps precision between 0 and 0.9999",
        ));
    }
    let fee_for_amount = |principal: Decimal| -> Result<Decimal> {
        Ok(calculate_dynamic_fee_breakdown(
            principal / price,
            price,
            fee_rate,
            fee_exponent,
            builder_share_bps,
        )?
        .total_fee)
    };
    if amount + fee_for_amount(amount)? <= user_usdc_balance {
        return Ok(amount);
    }

    let scale = Decimal::from(1_000_000_u32);
    let high_decimal = amount.min(user_usdc_balance) * scale;
    let mut high = high_decimal
        .trunc()
        .to_u64()
        .ok_or_else(|| Error::validation("market-buy amount does not fit in USDC units"))?;
    let mut low = 0_u64;
    let mut best = 0_u64;
    while low <= high {
        let mid = low + (high - low) / 2;
        let principal = Decimal::from(mid) / scale;
        if principal + fee_for_amount(principal)? <= user_usdc_balance {
            best = mid;
            low = mid.saturating_add(1);
        } else if mid == 0 {
            break;
        } else {
            high = mid - 1;
        }
    }

    if best == 0 {
        return Err(Error::validation(format!(
            "user_usdc_balance {user_usdc_balance} too small to cover fees at price {price}; \
             fee-adjusted amount truncated to zero"
        )));
    }
    Ok(Decimal::from(best) / scale)
}

fn decimal_pow(base: Decimal, exponent: u32) -> Result<Decimal> {
    let mut value = Decimal::ONE;
    for _ in 0..exponent {
        value = value
            .checked_mul(base)
            .ok_or_else(|| Error::validation("dynamic fee calculation overflow"))?;
    }
    Ok(value)
}

/// Validates that a price is within the valid range `[tick_size, 1 - tick_size]`.
#[must_use]
pub fn price_valid(price: Decimal, tick_size: TickSize) -> bool {
    let ts = Decimal::from(tick_size);
    price >= ts && price <= dec!(1) - ts
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use rust_decimal_macros::dec;

    use super::*;
    use crate::clob::types::response::BuilderFeeRateResponse;
    use crate::types::{B256, U256};

    fn make_orderbook(
        bids: Vec<OrderSummary>,
        asks: Vec<OrderSummary>,
    ) -> OrderBookSummaryResponse {
        OrderBookSummaryResponse::builder()
            .market(B256::ZERO)
            .asset_id(U256::ZERO)
            .timestamp(Utc::now())
            .bids(bids)
            .asks(asks)
            .min_order_size(dec!(0.01))
            .neg_risk(false)
            .tick_size(TickSize::Hundredth)
            .build()
    }

    fn order(price: Decimal, size: Decimal) -> OrderSummary {
        OrderSummary::builder().price(price).size(size).build()
    }

    #[test]
    fn calculate_market_price_buy_usdc_sufficient_liquidity() {
        // Asks are delivered worst-first on the wire, so the walk proceeds 0.50 → 0.51.
        let ob = make_orderbook(
            vec![],
            vec![
                order(dec!(0.52), dec!(100)),
                order(dec!(0.51), dec!(100)),
                order(dec!(0.50), dec!(100)),
            ],
        );
        // 0.50*100 = 50, 0.51*100 = 51 → 101 ≥ 80
        let amt = Amount::usdc(dec!(80)).unwrap();
        assert_eq!(
            calculate_market_price(&ob, Side::Buy, amt, &OrderType::FOK).unwrap(),
            dec!(0.51),
        );
    }

    #[test]
    fn calculate_market_price_buy_shares_sufficient_liquidity() {
        let ob = make_orderbook(
            vec![],
            vec![
                order(dec!(0.52), dec!(100)),
                order(dec!(0.51), dec!(100)),
                order(dec!(0.50), dec!(100)),
            ],
        );
        // 100, then 200 ≥ 150 → 0.51
        let amt = Amount::shares(dec!(150)).unwrap();
        assert_eq!(
            calculate_market_price(&ob, Side::Buy, amt, &OrderType::FOK).unwrap(),
            dec!(0.51),
        );
    }

    #[test]
    fn calculate_market_price_buy_insufficient_fok() {
        let ob = make_orderbook(vec![], vec![order(dec!(0.50), dec!(10))]);
        let amt = Amount::usdc(dec!(100)).unwrap();
        calculate_market_price(&ob, Side::Buy, amt, &OrderType::FOK).unwrap_err();
    }

    #[test]
    fn calculate_market_price_buy_insufficient_fak() {
        // Asks worst-first → 0.60 is levels[0]. FAK with insufficient liquidity
        // falls back to that worst price so the caller gets a safe upper bound.
        let ob = make_orderbook(
            vec![],
            vec![order(dec!(0.60), dec!(5)), order(dec!(0.50), dec!(10))],
        );
        let amt = Amount::usdc(dec!(1000)).unwrap();
        assert_eq!(
            calculate_market_price(&ob, Side::Buy, amt, &OrderType::FAK).unwrap(),
            dec!(0.60),
        );
    }

    #[test]
    fn calculate_market_price_sell_shares() {
        // Bids are delivered worst-first on the wire, so the walk proceeds 0.50 → 0.49.
        let ob = make_orderbook(
            vec![
                order(dec!(0.48), dec!(100)),
                order(dec!(0.49), dec!(100)),
                order(dec!(0.50), dec!(100)),
            ],
            vec![],
        );
        // 100, then 200 ≥ 150 → 0.49
        let amt = Amount::shares(dec!(150)).unwrap();
        assert_eq!(
            calculate_market_price(&ob, Side::Sell, amt, &OrderType::FOK).unwrap(),
            dec!(0.49),
        );
    }

    #[test]
    fn calculate_market_price_sell_usdc_is_rejected() {
        let ob = make_orderbook(
            vec![order(dec!(0.49), dec!(100))],
            vec![order(dec!(0.51), dec!(100))],
        );
        let amt = Amount::usdc(dec!(10)).unwrap();
        calculate_market_price(&ob, Side::Sell, amt, &OrderType::FOK).unwrap_err();
    }

    #[test]
    fn calculate_market_price_empty_orderbook() {
        let ob = make_orderbook(vec![], vec![]);
        let amt = Amount::usdc(dec!(100)).unwrap();
        calculate_market_price(&ob, Side::Buy, amt, &OrderType::FOK).unwrap_err();
    }

    #[test]
    fn calculate_market_price_unknown_side_errors() {
        let ob = make_orderbook(
            vec![order(dec!(0.49), dec!(100))],
            vec![order(dec!(0.51), dec!(100))],
        );
        let amt = Amount::usdc(dec!(10)).unwrap();
        calculate_market_price(&ob, Side::Unknown, amt, &OrderType::FOK).unwrap_err();
    }

    #[test]
    fn price_valid_within_bounds() {
        assert!(price_valid(dec!(0.5), TickSize::Hundredth));
        assert!(price_valid(dec!(0.01), TickSize::Hundredth));
        assert!(price_valid(dec!(0.99), TickSize::Hundredth));
    }

    #[test]
    fn price_valid_at_boundaries() {
        assert!(price_valid(dec!(0.1), TickSize::Tenth));
        assert!(price_valid(dec!(0.9), TickSize::Tenth));
    }

    #[test]
    fn price_valid_out_of_bounds() {
        assert!(!price_valid(dec!(0.0), TickSize::Hundredth));
        assert!(!price_valid(dec!(1.0), TickSize::Hundredth));
        assert!(!price_valid(dec!(0.005), TickSize::Hundredth));
        assert!(!price_valid(dec!(0.995), TickSize::Hundredth));
    }

    #[test]
    fn price_valid_all_tick_sizes() {
        assert!(price_valid(dec!(0.5), TickSize::Tenth));
        assert!(price_valid(dec!(0.5), TickSize::Hundredth));
        assert!(price_valid(dec!(0.5), TickSize::Thousandth));
        assert!(price_valid(dec!(0.5), TickSize::TenThousandth));
    }

    #[test]
    fn orderbook_hash_deterministic() {
        let ts = DateTime::from_timestamp_millis(1_700_000_000_000).expect("valid ts");
        let ob = OrderBookSummaryResponse::builder()
            .market(B256::ZERO)
            .asset_id(U256::ZERO)
            .timestamp(ts)
            .bids(vec![order(dec!(0.49), dec!(50))])
            .asks(vec![order(dec!(0.51), dec!(25))])
            .min_order_size(dec!(0.01))
            .neg_risk(false)
            .tick_size(TickSize::Hundredth)
            .build();

        let hash = orderbook_summary_hash(&ob);
        assert_eq!(hash.len(), 40);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(hash, orderbook_summary_hash(&ob));
    }

    #[test]
    fn orderbook_hash_differs_for_different_inputs() {
        let ts = DateTime::from_timestamp_millis(1_700_000_000_000).expect("valid ts");
        let ob1 = OrderBookSummaryResponse::builder()
            .market(B256::ZERO)
            .asset_id(U256::from(1_u64))
            .timestamp(ts)
            .min_order_size(dec!(0.01))
            .neg_risk(false)
            .tick_size(TickSize::Hundredth)
            .build();

        let ob2 = OrderBookSummaryResponse::builder()
            .market(B256::ZERO)
            .asset_id(U256::from(2_u64))
            .timestamp(ts)
            .min_order_size(dec!(0.01))
            .neg_risk(false)
            .tick_size(TickSize::Hundredth)
            .build();

        assert_ne!(orderbook_summary_hash(&ob1), orderbook_summary_hash(&ob2));
    }

    #[test]
    fn adjust_market_buy_no_adjustment_when_balance_sufficient() {
        let result = adjust_market_buy_amount(
            dec!(100),
            dec!(1000),
            dec!(0.5),
            dec!(0.02),
            dec!(1),
            dec!(0),
        )
        .unwrap();
        assert_eq!(result, dec!(100));
    }

    #[test]
    fn adjust_market_buy_adjusts_when_balance_insufficient() {
        let result = adjust_market_buy_amount(
            dec!(100),
            dec!(100),
            dec!(0.5),
            dec!(0.02),
            dec!(1),
            dec!(0),
        )
        .unwrap();
        assert!(result < dec!(100));
        assert!(result > dec!(0));
    }

    #[test]
    fn builder_share_without_kuest_fee_charges_zero() {
        let result = adjust_market_buy_amount(
            dec!(100),
            dec!(100),
            dec!(0.5),
            dec!(0),
            dec!(1),
            dec!(0.005),
        )
        .unwrap();
        assert_eq!(result, dec!(100));
    }

    #[test]
    fn adjust_market_buy_errors_when_balance_truncates_to_zero() {
        // user_usdc_balance smaller than 1e-6 after fee-divisor → truncates to zero.
        let err = adjust_market_buy_amount(
            dec!(100),       // wanted amount
            dec!(0.0000001), // balance well below 1 USDC-micro
            dec!(0.5),
            dec!(0.02),
            dec!(1),
            dec!(0.005),
        )
        .unwrap_err();
        assert!(err.to_string().contains("truncated to zero"));
    }

    // Fee calculation tests ported from TS `feeCalculations.test.ts`.

    /// `platform_fee = (amount / price) × rate × (price × (1 − price))^exponent`.
    fn calc_platform_fee(amount: Decimal, price: Decimal, rate: Decimal, exponent: u32) -> Decimal {
        let base = price * (Decimal::ONE - price);
        let rate_factor = rate * decimal_pow(base, exponent).unwrap();
        (amount / price) * rate_factor
    }

    fn calc_grossed_fee(
        amount: Decimal,
        price: Decimal,
        rate: Decimal,
        exponent: u32,
        builder_share: Decimal,
    ) -> Decimal {
        calc_platform_fee(amount, price, rate, exponent) / (Decimal::ONE - builder_share)
    }

    fn calc_exact_total_fee(
        amount: Decimal,
        price: Decimal,
        rate: Decimal,
        exponent: u32,
        builder_share_bps: u32,
    ) -> Decimal {
        calculate_dynamic_fee_breakdown(
            amount / price,
            price,
            rate,
            Decimal::from(exponent),
            builder_share_bps,
        )
        .unwrap()
        .total_fee
    }

    fn close_to(actual: Decimal, expected: Decimal, tol: Decimal) {
        let diff = (actual - expected).abs();
        assert!(
            diff <= tol,
            "|{actual} − {expected}| = {diff} exceeds tolerance {tol}"
        );
    }

    // Platform fee at representative prices (rate=0.25, exp=2, C=100 contracts).

    #[test]
    fn platform_fee_0_25_exp_2_at_midprice() {
        // price=0.5 → 1.5625
        close_to(
            calc_platform_fee(dec!(100) * dec!(0.5), dec!(0.5), dec!(0.25), 2),
            dec!(1.5625),
            dec!(0.000001),
        );
    }

    #[test]
    fn platform_fee_0_25_exp_2_symmetric_prices() {
        // (0.3, 0.7), (0.1, 0.9), (0.05, 0.95), (0.01, 0.99) must all pair up.
        let cases = [
            (dec!(0.3), dec!(0.7), dec!(1.1025)),
            (dec!(0.1), dec!(0.9), dec!(0.2025)),
            (dec!(0.05), dec!(0.95), dec!(0.05640625)),
            (dec!(0.01), dec!(0.99), dec!(0.00245025)),
        ];
        for (p_low, p_high, expected) in cases {
            close_to(
                calc_platform_fee(dec!(100) * p_low, p_low, dec!(0.25), 2),
                expected,
                dec!(0.000001),
            );
            close_to(
                calc_platform_fee(dec!(100) * p_high, p_high, dec!(0.25), 2),
                expected,
                dec!(0.000001),
            );
        }
    }

    #[test]
    fn platform_fee_0_25_exp_2_fractional_contracts() {
        // price=0.5, C=125.5 → 1.9609375
        close_to(
            calc_platform_fee(dec!(125.5) * dec!(0.5), dec!(0.5), dec!(0.25), 2),
            dec!(1.9609375),
            dec!(0.000001),
        );
    }

    #[test]
    fn grossed_fee_preserves_the_kuest_base() {
        let amount_usd = dec!(100) * dec!(0.5);
        let platform = calc_platform_fee(amount_usd, dec!(0.5), dec!(0.25), 2);
        let total = calc_grossed_fee(amount_usd, dec!(0.5), dec!(0.25), 2, dec!(0.3));
        close_to(platform, dec!(1.5625), dec!(0.000001));
        close_to(total * dec!(0.7), platform, dec!(0.000001));
    }

    // `adjust_market_buy_amount` boundary behaviour.

    #[test]
    fn adjust_buy_balance_strictly_greater_returns_amount_unchanged() {
        let amount = dec!(50);
        let price = dec!(0.5);
        let fee = calc_platform_fee(amount, price, dec!(0.25), 2);
        let balance = amount + fee + dec!(1); // comfortably above total cost
        let result =
            adjust_market_buy_amount(amount, balance, price, dec!(0.25), dec!(2), dec!(0)).unwrap();
        assert_eq!(result, amount);
    }

    #[test]
    fn adjust_buy_balance_equal_to_total_cost_matches_divide_path() {
        // TS boundary: at `balance == totalCost` the `<=` check fires and returns
        // `balance / divisor`, which equals the original amount by construction.
        let amount = dec!(50);
        let price = dec!(0.5);
        let fee = calc_platform_fee(amount, price, dec!(0.25), 2);
        let total_cost = amount + fee;
        let result =
            adjust_market_buy_amount(amount, total_cost, price, dec!(0.25), dec!(2), dec!(0))
                .unwrap();
        close_to(result, amount, dec!(0.000001));
    }

    #[test]
    fn adjust_buy_conserves_notional_platform_only() {
        // balance = amount (no room for fees): adjusted + fee must reconstitute `amount`.
        let amount = dec!(50);
        let price = dec!(0.5);
        let adjusted =
            adjust_market_buy_amount(amount, amount, price, dec!(0.25), dec!(2), dec!(0)).unwrap();
        let fee = calc_exact_total_fee(adjusted, price, dec!(0.25), 2, 0);
        assert!(adjusted + fee <= amount);
        assert!(amount - adjusted - fee < dec!(0.000002));
        assert!(adjusted < amount);
    }

    #[test]
    fn builder_share_without_a_platform_fee_charges_zero() {
        let amount = dec!(50);
        let price = dec!(0.5);
        let adjusted =
            adjust_market_buy_amount(amount, amount, price, dec!(0), dec!(0), dec!(0.3)).unwrap();
        assert_eq!(adjusted, amount);
    }

    #[test]
    fn adjust_buy_conserves_notional_platform_and_builder() {
        let amount = dec!(50);
        let price = dec!(0.5);
        let builder_share = dec!(0.3);
        let adjusted =
            adjust_market_buy_amount(amount, amount, price, dec!(0.25), dec!(2), builder_share)
                .unwrap();
        let total_fee = calc_exact_total_fee(adjusted, price, dec!(0.25), 2, 3_000);
        assert!(adjusted + total_fee <= amount);
        assert!(amount - adjusted - total_fee < dec!(0.000002));
    }

    #[test]
    fn adjust_buy_conserves_notional_at_price_0_3() {
        let amount = dec!(30);
        let price = dec!(0.3);
        let builder_share = dec!(0.45);
        let adjusted =
            adjust_market_buy_amount(amount, amount, price, dec!(0.25), dec!(2), builder_share)
                .unwrap();
        let total_fee = calc_exact_total_fee(adjusted, price, dec!(0.25), 2, 4_500);
        assert!(adjusted + total_fee <= amount);
        assert!(amount - adjusted - total_fee < dec!(0.000002));
    }

    // Kuest fee tiers (all exp=1).

    #[test]
    fn production_fee_sports_v2() {
        close_to(
            calc_platform_fee(dec!(100), dec!(0.5), dec!(0.0315), 1),
            dec!(1.575),
            dec!(0.000001),
        );
        close_to(
            calc_platform_fee(dec!(100), dec!(0.3), dec!(0.0315), 1),
            dec!(2.205),
            dec!(0.000001),
        );
        close_to(
            calc_platform_fee(dec!(100), dec!(0.7), dec!(0.0315), 1),
            dec!(0.945),
            dec!(0.000001),
        );
    }

    #[test]
    fn production_fee_politics_family() {
        // rate=0.0252, exp=1 — politics, tech, finance, mentions
        close_to(
            calc_platform_fee(dec!(100), dec!(0.5), dec!(0.0252), 1),
            dec!(1.26),
            dec!(0.000001),
        );
        close_to(
            calc_platform_fee(dec!(100), dec!(0.3), dec!(0.0252), 1),
            dec!(1.764),
            dec!(0.000001),
        );
        close_to(
            calc_platform_fee(dec!(100), dec!(0.7), dec!(0.0252), 1),
            dec!(0.756),
            dec!(0.000001),
        );
    }

    #[test]
    fn production_fee_culture_family() {
        // rate=0.0315, exp=1 — culture, weather, general, economics
        close_to(
            calc_platform_fee(dec!(100), dec!(0.5), dec!(0.0315), 1),
            dec!(1.575),
            dec!(0.000001),
        );
        close_to(
            calc_platform_fee(dec!(100), dec!(0.3), dec!(0.0315), 1),
            dec!(2.205),
            dec!(0.000001),
        );
        close_to(
            calc_platform_fee(dec!(100), dec!(0.7), dec!(0.0315), 1),
            dec!(0.945),
            dec!(0.000001),
        );
    }

    #[test]
    fn production_fee_crypto_v2() {
        // rate=0.0441, exp=1
        close_to(
            calc_platform_fee(dec!(100), dec!(0.5), dec!(0.0441), 1),
            dec!(2.205),
            dec!(0.000001),
        );
        close_to(
            calc_platform_fee(dec!(100), dec!(0.3), dec!(0.0441), 1),
            dec!(3.087),
            dec!(0.000001),
        );
        close_to(
            calc_platform_fee(dec!(100), dec!(0.7), dec!(0.0441), 1),
            dec!(1.323),
            dec!(0.000001),
        );
    }

    #[test]
    fn production_fee_geopolitics_matches_politics() {
        for (price, expected) in [
            (dec!(0.3), dec!(1.764)),
            (dec!(0.5), dec!(1.26)),
            (dec!(0.7), dec!(0.756)),
        ] {
            assert_eq!(
                calc_platform_fee(dec!(100), price, dec!(0.0252), 1),
                expected
            );
        }
    }

    #[test]
    fn production_crypto_golden_prices_for_100_shares() {
        for (price, expected_base) in [
            (dec!(0.01), dec!(0.04366)),
            (dec!(0.1), dec!(0.3969)),
            (dec!(0.5), dec!(1.1025)),
            (dec!(0.9), dec!(0.3969)),
            (dec!(0.99), dec!(0.04366)),
        ] {
            assert_eq!(
                calculate_dynamic_fee_breakdown(dec!(100), price, dec!(0.0441), dec!(1), 3_000)
                    .unwrap()
                    .kuest_fee_base,
                expected_base
            );
        }
    }

    #[test]
    fn production_category_midpoint_fixtures_for_100_shares() {
        for (rate, expected_base) in [
            (dec!(0.0441), dec!(1.1025)),
            (dec!(0.0315), dec!(0.7875)),
            (dec!(0.0252), dec!(0.63)),
        ] {
            assert_eq!(
                calculate_dynamic_fee_breakdown(dec!(100), dec!(0.5), rate, dec!(1), 3_000)
                    .unwrap()
                    .kuest_fee_base,
                expected_base
            );
        }
    }

    #[test]
    fn production_operator_share_and_fallback_fixtures() {
        for (share, total, operator, kuest) in [
            (2_000, dec!(1.37813), dec!(0.275626), dec!(1.102504)),
            (3_000, dec!(1.575), dec!(0.4725), dec!(1.1025)),
            (4_500, dec!(2.00455), dec!(0.902047), dec!(1.102503)),
        ] {
            let breakdown =
                calculate_dynamic_fee_breakdown(dec!(100), dec!(0.5), dec!(0.0441), dec!(1), share)
                    .unwrap();
            assert_eq!(breakdown.kuest_fee_base, dec!(1.1025));
            assert_eq!(breakdown.total_fee, total);
            assert_eq!(breakdown.operator_fee, operator);
            assert_eq!(breakdown.kuest_fee, kuest);
        }

        let fallback: BuilderFeeRateResponse = serde_json::from_str("{}").unwrap();
        assert_eq!(fallback.builder_taker_fee_share_bps, 3_000);
        assert_eq!(fallback.builder_maker_flat_fee_bps, 0);
    }

    #[test]
    fn builder_maker_flat_response_fixtures() {
        for maker_flat in [0, 100] {
            let response: BuilderFeeRateResponse = serde_json::from_value(serde_json::json!({
                "builder_maker_flat_fee_bps": maker_flat,
                "builder_taker_fee_share_bps": 3_000
            }))
            .unwrap();
            assert_eq!(response.builder_maker_flat_fee_bps, maker_flat);
            assert_eq!(response.builder_taker_fee_share_bps, 3_000);
        }
    }

    #[test]
    fn production_adjust_buy_conserves_notional_across_all_tiers() {
        // For every production tier at prices {0.3, 0.5, 0.7}, `adjust + fee ≈ amount`
        // when `balance == amount` (i.e. the budget is fully consumed).
        let amount = dec!(100);
        let tiers: [(&str, Decimal, u32); 5] = [
            ("sports_v2", dec!(0.0315), 1),
            ("politics_family", dec!(0.0252), 1),
            ("culture_family", dec!(0.0315), 1),
            ("crypto_v2", dec!(0.0441), 1),
            ("geopolitics", dec!(0.0252), 1),
        ];
        let prices = [dec!(0.3), dec!(0.5), dec!(0.7)];
        for (name, rate, exponent) in tiers {
            for price in prices {
                let adjusted = adjust_market_buy_amount(
                    amount,
                    amount,
                    price,
                    rate,
                    Decimal::from(exponent),
                    dec!(0.3),
                )
                .unwrap_or_else(|e| {
                    panic!("adjust failed for {name} @ price={price}: {e}");
                });
                let fee = calc_exact_total_fee(adjusted, price, rate, exponent, 3_000);
                let diff = amount - adjusted - fee;
                assert!(
                    diff >= Decimal::ZERO && diff < dec!(0.000002),
                    "tier={name} price={price}: adjusted ({adjusted}) + fee ({fee}) = {} vs \
                     amount {amount}, diff {diff}",
                    adjusted + fee,
                );
            }
        }
    }
}
