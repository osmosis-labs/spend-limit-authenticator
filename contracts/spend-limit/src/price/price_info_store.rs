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
    if let Some(price_info) = get_price(price_info_store, deps.as_ref(), conf, block_time, denom)? {
        price_info_store
            .save(deps.storage, denom, &price_info)
            .map_err(PriceError::StdError)?;

        return Ok(Some(price_info));
    };

    Ok(None)
}

pub fn get_price(
    price_info_store: &PriceInfoStore,
    deps: Deps,
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
    let price_info = fetch_twap_price(deps, conf, denom, block_time, price_info.swap_routes)?;

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

    let start_time = block_time.minus_nanos(conf.twap_duration.u64());
    let proto_start_time = to_proto_timestamp(start_time);
    let mut base_denom = base_denom.to_string();

    for route in swap_routes.iter() {
        let pool_id = route.pool_id;

        let arithmetic_twap = TwapQuerier::new(&deps.querier)
            .arithmetic_twap_to_now(
                pool_id,
                base_denom.clone(),
                route.token_out_denom.clone(),
                Some(proto_start_time.clone()),
            )
            .map_err(|_| {
                PriceError::twap_query_error(
                    pool_id,
                    base_denom.as_str(),
                    route.token_out_denom.as_str(),
                    start_time,
                )
            })?
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

fn valid_swap_routes(swap_routes: &[SwapAmountInRoute], quote_denom: &str) -> bool {
    if let Some(last_swap_route) = swap_routes.last() {
        last_swap_route.token_out_denom == quote_denom
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{ContractResult, OverflowError, OverflowOperation};
    use osmosis_std::types::osmosis::twap::v1beta1::ArithmeticTwapToNowResponse;
    use rstest::rstest;

    use crate::{
        state::PRICE_INFOS,
        test_helper::mock_stargate_querier::{
            arithmetic_twap_to_now_query_handler, mock_dependencies_with_stargate_querier,
        },
    };

    use super::*;

    const UUSDC: &str = "ibc/498A0751C798A0D9A389AA3691123DADA57DAA4FE165D5C75894505B876BA6E4";
    const UATOM: &str = "ibc/27394FB092D2ECCD56123C74F36E4C1F926001CEADA9CA97EA622B25F41E5EB2";

    #[test]
    fn test_track_denom() {
        let conf = PriceResolutionConfig {
            quote_denom: UUSDC.to_string(),
            staleness_threshold: 3_600_000_000_000u64.into(), // 1h
            twap_duration: 3_600_000_000_000u64.into(),       // 1h
        };
        let block_time = Timestamp::from_nanos(1_708_416_816_000_000_000);
        let expected_start_time =
            to_proto_timestamp(block_time.minus_nanos(conf.twap_duration.u64()));

        let mut deps = mock_dependencies_with_stargate_querier(
            &[],
            arithmetic_twap_to_now_query_handler(Box::new(move |req| {
                let base_asset = req.base_asset.as_str();
                let quote_asset = req.quote_asset.as_str();
                let start_time = req.start_time.clone().unwrap();

                if start_time != expected_start_time {
                    return ContractResult::Err(format!(
                        "expected start time: {:?}, got: {:?}",
                        expected_start_time, start_time
                    ));
                }

                let arithmetic_twap = match (base_asset, quote_asset) {
                    ("uosmo", UUSDC) => "1.500000000000000000",
                    _ => return ContractResult::Err("Price not found".to_string()),
                }
                .to_string();

                ContractResult::Ok(ArithmeticTwapToNowResponse { arithmetic_twap })
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
            swap_routes.clone(),
        )
        .unwrap();

        let price_info = PRICE_INFOS.load(deps.as_ref().storage, "uosmo").unwrap();
        assert_eq!(
            price_info,
            PriceInfo {
                price: "1.500000000000000000".parse::<Decimal>().unwrap(),
                last_updated_time: block_time,
                swap_routes
            }
        );
    }

    #[test]
    fn test_get_and_cache_price() {
        let conf = PriceResolutionConfig {
            quote_denom: UUSDC.to_string(),
            staleness_threshold: 3_600_000_000_000u64.into(), // 1h
            twap_duration: 3_600_000_000_000u64.into(),       // 1h
        };
        let last_updated_time = Timestamp::from_nanos(1_708_416_816_000_000_000);

        let mut deps = mock_dependencies_with_stargate_querier(
            &[],
            arithmetic_twap_to_now_query_handler(Box::new(move |req| {
                let base_asset = req.base_asset.as_str();
                let quote_asset = req.quote_asset.as_str();

                let arithmetic_twap = match (base_asset, quote_asset) {
                    ("uosmo", UUSDC) => "1.500000000000000000",
                    _ => return ContractResult::Err("Price not found".to_string()),
                }
                .to_string();

                ContractResult::Ok(ArithmeticTwapToNowResponse { arithmetic_twap })
            })),
        );

        let swap_routes = vec![SwapAmountInRoute {
            pool_id: 1,
            token_out_denom: UUSDC.to_string(),
        }];

        let cached_price_info = PriceInfo {
            price: "1.400000000000000000".parse::<Decimal>().unwrap(),
            last_updated_time,
            swap_routes: swap_routes.clone(),
        };

        // save cached price
        PRICE_INFOS
            .save(&mut deps.storage, "uosmo", &cached_price_info)
            .unwrap();

        // cache hit
        let price_info = get_and_cache_price(
            &PRICE_INFOS,
            deps.as_mut(),
            &conf,
            last_updated_time.plus_nanos(1_800_000_000_000u64), // + 30m
            "uosmo",
        )
        .unwrap();
        assert_eq!(price_info, Some(cached_price_info));

        // cache miss, update
        let price_info = get_and_cache_price(
            &PRICE_INFOS,
            deps.as_mut(),
            &conf,
            last_updated_time.plus_nanos(3_600_000_000_001u64), // + 1h + 1ns
            "uosmo",
        )
        .unwrap();
        assert_eq!(
            price_info,
            Some(PriceInfo {
                price: "1.500000000000000000".parse::<Decimal>().unwrap(),
                last_updated_time: last_updated_time.plus_nanos(3_600_000_000_001u64), // + 1h + 1ns
                swap_routes: swap_routes.clone()
            })
        );

        // cache hit updated one
        let price_info = get_and_cache_price(
            &PRICE_INFOS,
            deps.as_mut(),
            &conf,
            last_updated_time.plus_nanos(3_600_000_000_002u64), // + 1h + 2ns
            "uosmo",
        )
        .unwrap();
        assert_eq!(
            price_info,
            Some(PriceInfo {
                price: "1.500000000000000000".parse::<Decimal>().unwrap(),
                last_updated_time: last_updated_time.plus_nanos(3_600_000_000_001u64), // + 1h + 1ns
                swap_routes
            })
        );

        // get quote denom
        let price_info = get_and_cache_price(
            &PRICE_INFOS,
            deps.as_mut(),
            &conf,
            last_updated_time.plus_nanos(3_600_000_000_002u64), // + 1h + 2ns
            UUSDC,
        )
        .unwrap();
        assert_eq!(
            price_info,
            Some(PriceInfo {
                price: Decimal::one(),
                last_updated_time: last_updated_time.plus_nanos(3_600_000_000_002u64), // + 1h + 2ns
                swap_routes: vec![]
            })
        );

        // get non-tracked denom
        let price_info = get_and_cache_price(
            &PRICE_INFOS,
            deps.as_mut(),
            &conf,
            last_updated_time.plus_nanos(3_600_000_000_002u64), // + 1h + 2ns
            "uatom",
        )
        .unwrap();
        assert_eq!(price_info, None);
    }

    #[rstest]
    #[case::valid_swap_routes_ending_with_quote_denom(
        UATOM,
        UUSDC,
        vec![
            SwapAmountInRoute { pool_id: 1, token_out_denom: "uosmo".to_string() },
            SwapAmountInRoute { pool_id: 2, token_out_denom: UUSDC.to_string() }
        ],
        Ok("9.600000000000000000")
    )]
    #[case::swap_routes_not_ending_with_quote_denom(
        "uosmo",
        UUSDC,
        vec![
            SwapAmountInRoute { pool_id: 1, token_out_denom: UATOM.to_string() }
        ],
        Err(PriceError::SwapRoutesMustEndWithQuoteDenom { quote_denom: UUSDC.to_string(), swap_routes: swap_routes.clone() })
    )]
    #[case::empty_swap_routes(
        "uosmo",
        UUSDC,
        vec![],
        Err(PriceError::SwapRoutesMustEndWithQuoteDenom { quote_denom: UUSDC.to_string(), swap_routes: swap_routes.clone() })
    )]
    #[case::invalid_arithmetic_twap(
        "uany",
        "uinvalid",
        vec![
            SwapAmountInRoute { pool_id: 99, token_out_denom: "uinvalid".to_string() }
        ],
        Err(PriceError::StdError(cosmwasm_std::StdError::generic_err("Error parsing whole")))
    )]
    #[case::overflow_arithmetic_twap(
        "uany",
        "uoverflow",
        vec![
            SwapAmountInRoute { pool_id: 991, token_out_denom: "udecmax".to_string() },
            SwapAmountInRoute { pool_id: 992, token_out_denom: "uoverflow".to_string() }
        ],
        Err(PriceError::PriceCalculationError(
            OverflowError::new(OverflowOperation::Mul, Decimal::MAX, 2)
        ))
    )]

    fn test_fetch_twap_price(
        #[case] base_denom: &str,
        #[case] quote_denom: &str,
        #[case] swap_routes: Vec<SwapAmountInRoute>,
        #[case] expected: Result<&str, PriceError>,
    ) {
        let conf = PriceResolutionConfig {
            quote_denom: quote_denom.to_string(),
            staleness_threshold: 3_600_000_000_000u64.into(), // 1h
            twap_duration: 3_600_000_000_000u64.into(),       // 1h
        };
        let block_time = Timestamp::from_nanos(1_708_416_816_000_000_000);

        let deps = mock_dependencies_with_stargate_querier(
            &[], // No balances needed for this test
            arithmetic_twap_to_now_query_handler(Box::new(move |req| {
                let base_asset = req.base_asset.as_str();
                let quote_asset = req.quote_asset.as_str();
                let pool_id = req.pool_id;

                match (pool_id, base_asset, quote_asset) {
                    (1, UATOM, "uosmo") => ContractResult::Ok(ArithmeticTwapToNowResponse {
                        arithmetic_twap: "6.400000000000000000".to_string(),
                    }),
                    (2, "uosmo", UUSDC) => ContractResult::Ok(ArithmeticTwapToNowResponse {
                        arithmetic_twap: "1.500000000000000000".to_string(),
                    }),
                    (99, _, _) => ContractResult::Ok(ArithmeticTwapToNowResponse {
                        arithmetic_twap: "not_a_decimal".to_string(),
                    }),
                    (991, _, _) => ContractResult::Ok(ArithmeticTwapToNowResponse {
                        arithmetic_twap: Decimal::MAX.to_string(),
                    }),
                    (992, _, _) => ContractResult::Ok(ArithmeticTwapToNowResponse {
                        arithmetic_twap: "2.000000000000000000".to_string(),
                    }),
                    _ => ContractResult::Err("Price not found".to_string()),
                }
            })),
        );

        let result = fetch_twap_price(
            deps.as_ref(),
            &conf,
            base_denom,
            block_time,
            swap_routes.clone(),
        );

        match expected {
            Ok(expected) => assert_eq!(
                result.unwrap(),
                PriceInfo {
                    price: expected.parse::<Decimal>().unwrap(),
                    last_updated_time: block_time,
                    swap_routes
                }
            ),
            Err(e) => assert_eq!(result.unwrap_err(), e),
        }
    }

    #[rstest]
    #[case(Timestamp::from_nanos(1_708_416_816_000_000_000), ProtoTimestamp { seconds: 1708416816, nanos: 0 })]
    #[case(Timestamp::from_nanos(1_609_459_200_000_000_000), ProtoTimestamp { seconds: 1609459200, nanos: 0 })]
    #[case(Timestamp::from_nanos(1_708_416_816_500_000_000), ProtoTimestamp { seconds: 1708416816, nanos: 500000000 })]
    #[case(Timestamp::from_nanos(1_609_459_200_250_000_001), ProtoTimestamp { seconds: 1609459200, nanos: 250000001 })]
    #[case(Timestamp::from_nanos(1_509_495_600_750_000_002), ProtoTimestamp { seconds: 1509495600, nanos: 750000002 })]
    #[case(Timestamp::from_nanos(1_409_532_000_125_000_003), ProtoTimestamp { seconds: 1409532000, nanos: 125000003 })]
    #[case(Timestamp::from_nanos(1_309_568_400_875_000_004), ProtoTimestamp { seconds: 1309568400, nanos: 875000004 })]
    fn test_to_proto_timestamp(#[case] input: Timestamp, #[case] expected: ProtoTimestamp) {
        let result = to_proto_timestamp(input);
        assert_eq!(result, expected);
    }
}
