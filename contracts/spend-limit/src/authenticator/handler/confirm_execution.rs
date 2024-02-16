use cosmwasm_std::{DepsMut, Env, Response};
use osmosis_authenticators::{ConfirmExecutionRequest, ConfirmationResult};

use crate::price::get_and_cache_price;
use crate::spend_limit::{calculate_spent_coins, SpendLimitError, SpendLimitParams};

use crate::state::{PRE_EXEC_BALANCES, PRICE_INFOS, PRICE_RESOLUTION_CONFIG, SPENDINGS};
use crate::ContractError;

use super::validate_and_parse_params;

pub fn confirm_execution(
    mut deps: DepsMut,
    env: Env,
    ConfirmExecutionRequest {
        account,
        authenticator_params,
        ..
    }: ConfirmExecutionRequest,
) -> Result<Response, ContractError> {
    let params: SpendLimitParams = validate_and_parse_params(authenticator_params)?;

    let spend_limit_key = (&account, params.subkey.as_str());

    // get the pre_exec balance for this key
    let pre_exec_balances = PRE_EXEC_BALANCES.load(deps.storage, spend_limit_key)?;

    // query all the balances of the account
    let post_exec_balances = deps.querier.query_all_balances(&account)?;

    let pre_exec_balances = pre_exec_balances.try_into()?;
    let post_exec_balances = post_exec_balances.try_into()?;
    let spent_coins = calculate_spent_coins(pre_exec_balances, post_exec_balances)?;

    let mut spending = SPENDINGS.load(deps.storage, spend_limit_key)?;

    let conf = PRICE_RESOLUTION_CONFIG.load(deps.storage)?;

    for coin in spent_coins.iter() {
        // If the coin is not tracked, we don't count it towards the spending limit
        let Some(price_info) = get_and_cache_price(
            &PRICE_INFOS,
            deps.branch(),
            &conf,
            env.block.time,
            &coin.denom,
        )?
        else {
            continue;
        };

        match spending.spend(
            coin.amount,
            price_info.price,
            params.limit.amount,
            &params.reset_period,
            env.block.time,
        ) {
            Err(overspent @ SpendLimitError::Overspend { .. }) => {
                return Ok(Response::new().set_data(ConfirmationResult::Block {
                    msg: overspent.to_string(),
                }));
            }
            otherwise => otherwise?,
        };
    }

    // save the updated spending
    SPENDINGS.save(deps.storage, spend_limit_key, &spending)?;

    // clean up the pre_exec balance
    PRE_EXEC_BALANCES.remove(deps.storage, spend_limit_key);

    Ok(Response::new().set_data(ConfirmationResult::Confirm {}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        price::PriceResolutionConfig,
        spend_limit::{Period, SpendLimitParams, Spending},
    };
    use cosmwasm_std::{
        testing::{mock_dependencies_with_balances, mock_env},
        to_json_binary, Addr, Binary, Coin, Response,
    };
    use osmosis_authenticators::ConfirmExecutionRequest;
    use rstest::rstest;

    #[rstest]
    #[case::spend_at_limit(1000, 500, 500, ConfirmationResult::Confirm {})]
    #[case::spend_over_limit(1000, 500, 501, ConfirmationResult::Block { msg: SpendLimitError::overspend(500, 501).to_string() })]
    fn test_confirm_execution_only_spends_quoted_denom(
        #[case] initial_balance: u128,
        #[case] limit: u128,
        #[case] spent: u128,
        #[case] expected: ConfirmationResult,
    ) {
        let fixed_balance = Coin::new(500, "uosmo");
        // Setup the environment
        let mut deps = mock_dependencies_with_balances(&[(
            "account",
            &[
                Coin::new(initial_balance - spent, "uusdc"),
                fixed_balance.clone(),
            ],
        )]);

        let key = (&Addr::unchecked("account"), "subkey1");

        PRE_EXEC_BALANCES
            .save(
                deps.as_mut().storage,
                key,
                &vec![Coin::new(initial_balance, "uusdc"), fixed_balance],
            )
            .unwrap();

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

        // Confirm the execution
        let confirm_execution_request = ConfirmExecutionRequest {
            account: Addr::unchecked("account"),
            authenticator_params: Some(
                to_json_binary(&SpendLimitParams {
                    subkey: "subkey1".to_string(),
                    limit: Coin::new(limit, "uusdc"),
                    reset_period: Period::Day,
                })
                .unwrap(),
            ),
            msg: osmosis_authenticators::Any {
                type_url: "".to_string(),
                value: Binary::default(),
            },
        };

        let res = confirm_execution(deps.as_mut(), mock_env(), confirm_execution_request);
        match expected {
            ConfirmationResult::Confirm {} => {
                assert_eq!(
                    res.unwrap(),
                    Response::new().set_data(ConfirmationResult::Confirm {})
                );

                // Verify that the spending is updated correctly
                let spending = SPENDINGS.load(deps.as_ref().storage, key).unwrap();
                assert_eq!(
                    spending,
                    Spending {
                        value_spent_in_period: spent.into(),
                        last_spent_at: mock_env().block.time
                    }
                );
            }
            ConfirmationResult::Block { msg } => {
                let res = res.unwrap();
                assert_eq!(
                    res,
                    Response::new().set_data(ConfirmationResult::Block { msg })
                );
            }
        }
    }
}
