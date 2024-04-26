use cosmwasm_std::{Coins, DepsMut, Env, Response, Timestamp};
use osmosis_authenticators::AuthenticationRequest;

use crate::{
    fee::get_account_spending_fee,
    spend_limit::update_and_check_spend_limit,
    state::{PRICE_INFOS, PRICE_RESOLUTION_CONFIG, SPENDINGS, UNTRACKED_SPENT_FEES},
    ContractError,
};

use super::validate_and_parse_params;

pub fn authenticate(
    deps: DepsMut,
    env: Env,
    auth_request: AuthenticationRequest,
) -> Result<Response, ContractError> {
    let params = validate_and_parse_params(auth_request.authenticator_params)?;

    if let Some(time_limit) = params.time_limit {
        let start = time_limit.start.unwrap_or(Timestamp::from_nanos(0));
        let end = time_limit.end;

        let current = env.block.time;

        if !(start <= current && current <= end) {
            return Err(ContractError::NotWithinTimeLimit {
                current: env.block.time,
                start: time_limit.start,
                end: time_limit.end,
            });
        }
    }

    let key = (
        &auth_request.account,
        auth_request.authenticator_id.as_str(),
    );

    let mut spending = SPENDINGS.load(deps.storage, key)?;
    let untracked_spent_fee = UNTRACKED_SPENT_FEES
        .may_load(deps.storage, key)?
        .unwrap_or_default()
        .get_or_reset_accum_fee(&params.reset_period, env.block.time)?;
    let conf = PRICE_RESOLUTION_CONFIG.load(deps.storage)?;

    let account_spending_fee = get_account_spending_fee(
        &auth_request.account,
        &auth_request.fee_payer,
        auth_request.fee_granter.as_ref(),
        auth_request.fee,
    );

    // check whether the fee spent + about to spend is within the limit
    // this will not be committed to the state
    let coins = [untracked_spent_fee, account_spending_fee].concat();
    update_and_check_spend_limit(
        deps,
        &PRICE_INFOS,
        &mut spending,
        coins,
        Coins::default(),
        &conf,
        params.limit,
        &params.reset_period,
        env.block.time,
    )?;

    Ok(Response::new().add_attribute("action", "authenticate"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fee::UntrackedSpentFee;
    use crate::period::Period;
    use crate::price::{track_denom, PriceResolutionConfig};
    use crate::spend_limit::{SpendLimitError, Spending};
    use crate::spend_limit::{SpendLimitParams, TimeLimit};
    use crate::test_helper::mock_stargate_querier::{
        arithmetic_twap_to_now_query_handler, mock_dependencies_with_stargate_querier,
    };

    use crate::state::PRICE_INFOS;
    use cosmwasm_std::{
        testing::{mock_dependencies_with_balances, mock_env},
        to_json_binary, Addr, Binary, Coin, ContractResult, Timestamp,
    };
    use osmosis_authenticators::{Any, SignModeTxData, SignatureData, TxData};
    use osmosis_std::types::osmosis::poolmanager::v1beta1::SwapAmountInRoute;
    use osmosis_std::types::osmosis::twap::v1beta1::ArithmeticTwapToNowResponse;
    use rstest::rstest;

    #[rstest]
    #[case::no_time_limit(0, None, true)]
    #[case::no_time_limit(1_771_797_419_879_305_533, None, true)]
    #[case::no_time_limit(u64::MAX, None, true)]
    #[case::within_time_limit(1_771_797_419_879_305_533, Some((Some(current), current + 1)), true)]
    #[case::within_time_limit(1_771_797_419_879_305_533, Some((Some(current), current)), true)]
    #[case::within_time_limit(1_771_797_419_879_305_533, Some((None, current)), true)]
    #[case::not_within_time_limit(1_771_797_419_879_305_533, Some((Some(current), current - 1)), false)]
    #[case::not_within_time_limit(1_771_797_419_879_305_533, Some((Some(current + 1), current)), false)]
    #[case::not_within_time_limit(1_771_797_419_879_305_533, Some((None, current - 1)), false)]
    fn test_authenticate_time_limit(
        #[case] current: u64,
        #[case] time_limit: Option<(Option<u64>, u64)>,
        #[case] expected: bool,
    ) {
        // Setup the environment
        let mut deps = mock_dependencies_with_balances(&[("addr", &[])]);

        let key = (&Addr::unchecked("addr"), "2");

        SPENDINGS
            .save(&mut deps.storage, key, &Spending::default())
            .unwrap();

        PRICE_RESOLUTION_CONFIG
            .save(
                deps.as_mut().storage,
                &PriceResolutionConfig {
                    quote_denom: "uusdc".to_string(),
                    staleness_threshold: 3_600_000_000_000u64.into(), // 1h
                    twap_duration: 3_600_000_000_000u64.into(),       // 1h
                },
            )
            .unwrap();

        let time_limit = time_limit.map(|(start, end)| TimeLimit {
            start: start.map(Timestamp::from_nanos),
            end: Timestamp::from_nanos(end),
        });

        let request = AuthenticationRequest {
            authenticator_id: "2".to_string(),
            account: Addr::unchecked("addr"),
            fee_payer: Addr::unchecked("addr"),
            fee_granter: None,
            fee: vec![],
            authenticator_params: Some(
                to_json_binary(&SpendLimitParams {
                    limit: 1000u128.into(),
                    reset_period: Period::Day,
                    time_limit: time_limit.clone(),
                })
                .unwrap(),
            ),
            msg: Any {
                type_url: "".to_string(),
                value: Binary::default(),
            },
            msg_index: 0,
            signature: Binary::default(),
            sign_mode_tx_data: SignModeTxData {
                sign_mode_direct: Binary::default(),
                sign_mode_textual: None,
            },
            tx_data: TxData {
                chain_id: "osmosis-1".to_string(),
                account_number: 0,
                sequence: 0,
                timeout_height: 0,
                msgs: vec![],
                memo: "".to_string(),
            },
            signature_data: SignatureData {
                signers: vec![],
                signatures: vec![],
            },
            simulate: false,
        };

        let mut env = mock_env();

        env.block.time = Timestamp::from_nanos(current);

        let response = authenticate(deps.as_mut(), env.clone(), request);

        if expected {
            response.expect("expected authenticated");
        } else {
            let TimeLimit { start, end } = time_limit.unwrap();
            assert_eq!(
                response.unwrap_err(),
                ContractError::NotWithinTimeLimit {
                    current: env.block.time,
                    start,
                    end,
                }
            );
        }
    }

    #[rstest]
    #[case::no_fee_spent("account", None, vec![], vec![], Ok(()))]
    #[case::fee_spent_to_the_limit("account", None, vec![Coin::new(1_000_000_000, "uusdc")], vec![], Ok(()))]
    #[case::fee_spent_to_the_limit("account", None, vec![Coin::new(666_666_666, "uosmo")], vec![], Ok(()))]
    #[case::fee_spent_to_the_limit_with_untracked_denoms("account", None,
        vec![Coin::new(333_333_333, "uosmo"), Coin::new(500_000_000, "untracked1")],
        vec![Coin::new(500_000_000, "uusdc"), Coin::new(500_000_000, "untracked2")],
        Ok(()))
    ]
    #[case::fee_spent_over_the_limit("account", None,
        vec![Coin::new(1_000_000_001, "uusdc")],
        vec![],
        Err(SpendLimitError::overspend(1_000_000_000, 1_000_000_001).into()))
    ]
    #[case::fee_spent_over_the_limit("account", None,
        vec![Coin::new(666_666_668, "uosmo")],
        vec![],
        Err(SpendLimitError::overspend(1_000_000_000, 1_000_000_002).into()))
    ]
    #[case::fee_spent_over_the_limit("account", None,
        vec![Coin::new(1_000_000_001, "uusdc")],
        vec![],
        Err(SpendLimitError::overspend(1_000_000_000, 1_000_000_001).into()))
    ]
    #[case::fee_spent_over_the_limit("account", None,
        vec![Coin::new(333_333_333, "uosmo"), Coin::new(500_000_001, "uusdc")],
        vec![],
        Err(SpendLimitError::overspend(1_000_000_000, 1_000_000_001).into()))
    ]
    #[case::fee_spent_over_the_limit("account", None,
        vec![Coin::new(333_333_333, "uosmo")],
        vec![Coin::new(500_000_001, "uusdc")],
        Err(SpendLimitError::overspend(1_000_000_000, 1_000_000_001).into()))
    ]
    #[case::fee_spent_over_the_limit_by_fee_grant("account", Some("granter"),
        vec![Coin::new(1_000_000_001, "uusdc")],
        vec![],
        Ok(()))
    ]
    #[case::fee_spent_over_the_limit_by_fee_grant_and_untracked_fee("account", Some("granter"),
        vec![Coin::new(1_000_000_001, "uusdc")],
        vec![Coin::new(1_000_000_001, "uusdc")],
        Err(SpendLimitError::overspend(1_000_000_000, 1_000_000_001).into()))
    ]
    #[case::fee_spent_over_the_limit_by_fee_grant_and_untracked_fee("non_account", Some("granter"),
        vec![Coin::new(1_000_000_001, "uusdc")],
        vec![Coin::new(1_000_000_001, "uusdc")],
        Err(SpendLimitError::overspend(1_000_000_000, 1_000_000_001).into()))
    ]
    #[case::fee_spent_over_the_limit_with_granter_as_account("account", Some("account"),
        vec![Coin::new(1_000_000_001, "uusdc")],
        vec![],
        Err(SpendLimitError::overspend(1_000_000_000, 1_000_000_001).into()))
    ]
    #[case::fee_spent_over_the_limit_with_granter_as_account("non_account", Some("account"),
        vec![Coin::new(1_000_000_001, "uusdc")],
        vec![],
        Err(SpendLimitError::overspend(1_000_000_000, 1_000_000_001).into()))
    ]
    #[case::fee_spent_over_the_limit_by_non_account_fee_payer("non_account", None,
        vec![Coin::new(1_000_000_001, "uusdc")],
        vec![],
        Ok(()))
    ]
    #[case::fee_spent_over_the_limit_by_non_account_fee_payer_and_untracked_fee("non_account", None,
        vec![Coin::new(1_000_000_001, "uusdc")],
        vec![Coin::new(1_000_000_001, "uusdc")],
        Err(SpendLimitError::overspend(1_000_000_000, 1_000_000_001).into()))
    ]
    fn test_authenticate_fee_spent(
        #[case] fee_payer: &str,
        #[case] fee_granter: Option<&str>,
        #[case] fee: Vec<Coin>,
        #[case] untracked_spent_fee: Vec<Coin>,
        #[case] result: Result<(), ContractError>,
    ) {
        // Setup the environment

        let mut deps = mock_dependencies_with_stargate_querier(
            &[],
            arithmetic_twap_to_now_query_handler(Box::new(|req| {
                let base_asset = req.base_asset.as_str();
                let quote_asset = req.quote_asset.as_str();

                let arithmetic_twap = match (base_asset, quote_asset) {
                    ("uosmo", "uusdc") => "1.5",
                    _ => return ContractResult::Err("Price not found".to_string()),
                }
                .to_string();

                ContractResult::Ok(ArithmeticTwapToNowResponse { arithmetic_twap })
            })),
        );

        let key = (&Addr::unchecked("account"), "2");

        SPENDINGS
            .save(&mut deps.storage, key, &Spending::default())
            .unwrap();

        let conf = PriceResolutionConfig {
            quote_denom: "uusdc".to_string(),
            staleness_threshold: 3_600_000_000_000u64.into(), // 1h
            twap_duration: 3_600_000_000_000u64.into(),       // 1h
        };
        PRICE_RESOLUTION_CONFIG
            .save(deps.as_mut().storage, &conf)
            .unwrap();

        track_denom(
            &PRICE_INFOS,
            deps.as_mut(),
            &conf,
            "uosmo",
            mock_env().block.time,
            vec![SwapAmountInRoute {
                pool_id: 666,
                token_out_denom: "uusdc".to_string(),
            }],
        )
        .unwrap();

        if !untracked_spent_fee.is_empty() {
            UNTRACKED_SPENT_FEES
                .save(
                    &mut deps.storage,
                    key,
                    &UntrackedSpentFee {
                        fee: untracked_spent_fee,
                        updated_at: mock_env().block.time,
                    },
                )
                .unwrap();
        }

        let request = AuthenticationRequest {
            authenticator_id: "2".to_string(),
            account: Addr::unchecked("account"),
            fee_payer: Addr::unchecked(fee_payer),
            fee_granter: fee_granter.map(Addr::unchecked),
            fee,
            authenticator_params: Some(
                to_json_binary(&SpendLimitParams {
                    limit: 1_000_000_000u128.into(),
                    reset_period: Period::Day,
                    time_limit: None,
                })
                .unwrap(),
            ),
            msg: Any {
                type_url: "".to_string(),
                value: Binary::default(),
            },
            msg_index: 0,
            signature: Binary::default(),
            sign_mode_tx_data: SignModeTxData {
                sign_mode_direct: Binary::default(),
                sign_mode_textual: None,
            },
            tx_data: TxData {
                chain_id: "osmosis-1".to_string(),
                account_number: 0,
                sequence: 0,
                timeout_height: 0,
                msgs: vec![],
                memo: "".to_string(),
            },
            signature_data: SignatureData {
                signers: vec![],
                signatures: vec![],
            },
            simulate: false,
        };

        let response = authenticate(deps.as_mut(), mock_env(), request).map(|_| ());

        assert_eq!(response, result);
    }
}
