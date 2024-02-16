use std::str::FromStr;

use cosmwasm_std::{ensure, Decimal, Deps, DepsMut, StdError, Timestamp, Uint128};
use cw_storage_plus::Map;
use osmosis_std::shim::Timestamp as ProtoTimestamp;
use osmosis_std::types::osmosis::{
    poolmanager::v1beta1::SwapAmountInRoute, twap::v1beta1::TwapQuerier,
};
use time::OffsetDateTime;

use super::price_info::PriceInfo;
use super::{PriceError, PriceResolutionConfig};

pub type PriceInfoStore<'a> = Map<'a, &'a str, PriceInfo>;

pub fn track_denom(
    price_info_store: &PriceInfoStore,
    deps: DepsMut,
    conf: &PriceResolutionConfig,
    denom: &str,
    block_time: Timestamp,
    swap_routes: Vec<SwapAmountInRoute>,
) -> Result<(), PriceError> {
    let price_info = fetch_twap_price(deps.as_ref(), conf, denom, block_time, swap_routes)?;
    price_info_store
        .save(deps.storage, denom, &price_info)
        .map_err(PriceError::StdError)
}

pub fn get_and_cache_price(
    price_info_store: &PriceInfoStore,
    deps: DepsMut,
    conf: &PriceResolutionConfig,
    block_time: Timestamp,
    denom: &str,
) -> Result<Option<PriceInfo>, PriceError> {
    // if no cached price, it means that it's not tracked, return None
    let Some(price_info) = price_info_store.may_load(deps.storage, denom)? else {
        return Ok(None);
    };

    // if cached price is not over staleness threshold, return it
    if !price_info.has_expired(block_time, conf.staleness_threshold)? {
        return Ok(Some(price_info));
    }

    // else fetch the new price and cache it
    let price_info = fetch_twap_price(
        deps.as_ref(),
        conf,
        denom,
        block_time,
        price_info.swap_routes,
    )?;
    price_info_store.save(deps.storage, denom, &price_info)?;

    Ok(Some(price_info))
}

fn fetch_twap_price(
    deps: Deps,
    conf: &PriceResolutionConfig,
    base_denom: &str,
    block_time: Timestamp,
    swap_routes: Vec<SwapAmountInRoute>,
) -> Result<PriceInfo, PriceError> {
    // Ensure that the swap routes end with the quote denom
    ensure!(
        valid_swap_routes(&swap_routes, conf.quote_denom.as_str()),
        PriceError::SwapRoutesMustEndWithQuoteDenom {
            quote_denom: conf.quote_denom.to_string(),
            swap_routes
        }
    );

    let start_time = block_time.minus_nanos(conf.twap_duration.u64());
    let start_time = to_proto_timestamp(start_time)?;

    let mut base_asset = base_denom.to_string();

    let mut price = Decimal::one();

    for route in swap_routes.iter() {
        let pool_id = route.pool_id;

        // TODO: optimize this using direct mut ArithmeticTwapToNow request for no clone
        let arithmetic_twap = TwapQuerier::new(&deps.querier)
            .arithmetic_twap_to_now(
                pool_id,
                base_asset,
                conf.quote_denom.clone(),
                Some(start_time.clone()),
            )?
            .arithmetic_twap;

        // arithmetic_twap is a string representation of LegacyDec
        let arithmetic_twap = from_legacy_dec_str(&arithmetic_twap)?;

        price = price.checked_mul(arithmetic_twap)?;
        base_asset = route.token_out_denom.clone();
    }

    Ok(PriceInfo {
        price,
        last_updated_time: block_time,
        swap_routes,
    })
}

fn to_proto_timestamp(timestamp: Timestamp) -> Result<ProtoTimestamp, PriceError> {
    OffsetDateTime::from_unix_timestamp_nanos(timestamp.nanos() as i128)
        .map(|t| ProtoTimestamp {
            seconds: t.second() as i64,
            nanos: t.nanosecond() as i32,
        })
        .map_err(PriceError::TimestampConversionError)
}

// TODO: check the remaining paths? only if twap does not check that
fn valid_swap_routes(swap_routes: &[SwapAmountInRoute], quote_denom: &str) -> bool {
    if let Some(last_swap_route) = swap_routes.last() {
        last_swap_route.token_out_denom == quote_denom
    } else {
        false
    }
}

/// Convert a Cosmos SDK's LegacyDec string to a Decimal
/// LegacyDec string is a string of u128 with 18 decimal places
/// which matches the precision of [`Decimal`] in cosmwasm_std
fn from_legacy_dec_str(s: &str) -> Result<Decimal, StdError> {
    Uint128::from_str(s).map(Decimal::new)
}

// TODO:
// - [x] proper error handling
// - write test
//    - start with integration test and drive that down the line
// - remove price oracle contract address
// - wiring
//   - instantiate with price infos
//   - remove qoute denom from params and use state
// - documentation
