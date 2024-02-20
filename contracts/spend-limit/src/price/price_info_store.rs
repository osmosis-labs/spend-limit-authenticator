use cosmwasm_std::{ensure, Decimal, Deps, DepsMut, Timestamp};
use cw_storage_plus::Map;
use osmosis_std::shim::Timestamp as ProtoTimestamp;
use osmosis_std::types::osmosis::{
    poolmanager::v1beta1::SwapAmountInRoute, twap::v1beta1::TwapQuerier,
};

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
    // if denom is quote denom, return 1
    if denom == conf.quote_denom.as_str() {
        return Ok(Some(PriceInfo {
            price: Decimal::one(),
            last_updated_time: block_time,
            swap_routes: vec![],
        }));
    }

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

    // swap_routes will never be empty, as checked in the above function
    // so price will never remain 1 implicitly
    let mut price = Decimal::one();

    let start_time = to_proto_timestamp(block_time.minus_nanos(conf.twap_duration.u64()));
    let mut base_denom = base_denom.to_string();

    for route in swap_routes.iter() {
        let pool_id = route.pool_id;

        let arithmetic_twap = TwapQuerier::new(&deps.querier)
            .arithmetic_twap_to_now(
                pool_id,
                base_denom,
                route.token_out_denom.clone(),
                Some(start_time.clone()),
            )?
            .arithmetic_twap;

        price = price.checked_mul(arithmetic_twap.parse()?)?;
        base_denom = route.token_out_denom.clone();
    }

    Ok(PriceInfo {
        price,
        last_updated_time: block_time,
        swap_routes,
    })
}

fn to_proto_timestamp(timestamp: Timestamp) -> ProtoTimestamp {
    ProtoTimestamp {
        seconds: timestamp.seconds() as i64,
        nanos: timestamp.subsec_nanos() as i32,
    }
}

// TODO: check the remaining paths? only if twap does not check that
fn valid_swap_routes(swap_routes: &[SwapAmountInRoute], quote_denom: &str) -> bool {
    if let Some(last_swap_route) = swap_routes.last() {
        last_swap_route.token_out_denom == quote_denom
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use osmosis_std::types::osmosis::twap::v1beta1::ArithmeticTwapToNowResponse;

    use crate::{
        state::PRICE_INFOS,
        test_helper::mock_stargate_querier::{
            arithmetic_twap_to_now_query_handler, mock_dependencies_with_stargate_querier,
        },
    };

    use super::*;

    const UUSDC: &str = "ibc/498A0751C798A0D9A389AA3691123DADA57DAA4FE165D5C75894505B876BA6E4";

    #[test]
    fn test_track_denom() {
        let conf = PriceResolutionConfig {
            quote_denom: UUSDC.to_string(),
            staleness_threshold: 3_600_000_000_000u64.into(), // 1h
            twap_duration: 3_600_000_000_000u64.into(),       // 1h
        };
        let block_time = Timestamp::from_nanos(1708416816_000000000);
        let expected_start_time =
            to_proto_timestamp(block_time.minus_nanos(conf.twap_duration.u64()));

        let mut deps = mock_dependencies_with_stargate_querier(
            &[],
            arithmetic_twap_to_now_query_handler(Box::new(move |req| {
                let base_asset = req.base_asset.as_str();
                let quote_asset = req.quote_asset.as_str();
                let start_time = req.start_time.clone().unwrap();

                if start_time != expected_start_time {
                    panic!("expected start time");
                }

                let arithmetic_twap = match (base_asset, quote_asset) {
                    ("uosmo", UUSDC) => "1.500000000000000000",
                    _ => panic!("unexpected request: {:?}", req),
                }
                .to_string();

                ArithmeticTwapToNowResponse { arithmetic_twap }
            })),
        );

        let swap_routes = vec![SwapAmountInRoute {
            pool_id: 1,
            token_out_denom: UUSDC.to_string(),
        }];

        track_denom(
            &PRICE_INFOS,
            deps.as_mut(),
            &conf,
            "uosmo",
            block_time,
            swap_routes,
        )
        .unwrap();

        let price_info = PRICE_INFOS.load(deps.as_ref().storage, "uosmo").unwrap();
        assert_eq!(
            price_info.price,
            "1.500000000000000000".parse::<Decimal>().unwrap()
        );
    }
}
