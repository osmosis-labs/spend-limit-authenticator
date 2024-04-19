use std::cmp::max;

use cosmwasm_std::{Addr, Coin, DepsMut, StdError, Timestamp, Uint128};

use crate::{
    period::Period,
    price::{get_and_cache_price, PriceResolutionConfig},
    spend_limit::Spending,
    state::PRICE_INFOS,
    ContractError,
};

/// Get the spending fee for an account. If the account is not the fee payer, it does not count towards the spending limit.
pub fn get_account_spending_fee(
    account: &Addr,
    fee_payer: &Addr,
    fee_granter: Option<&Addr>,
    fee: Vec<Coin>,
) -> Vec<Coin> {
    // fee granter pay for the fee if specified
    let fee_payer = fee_granter.unwrap_or(fee_payer);

    // count fee paid towards this account only if the account is the fee payer
    if account == fee_payer {
        fee
    } else {
        vec![]
    }
}

pub fn update_and_check_spend_limit(
    mut deps: DepsMut,
    spending: &mut Spending,
    spent_coins: impl IntoIterator<Item = Coin>,
    received_coins: impl IntoIterator<Item = Coin>,
    conf: &PriceResolutionConfig,
    limit: Uint128,
    reset_period: &Period,
    time: Timestamp,
) -> Result<(), ContractError> {
    let prev_value_spent = spending.get_or_reset_value_spent(reset_period, time)?;
    let mut value_spent = prev_value_spent.clone();

    for spent in spent_coins.into_iter() {
        // If the coin is not tracked (hence quoted_value = None), we don't count it towards the spending limit
        let Some(spent_coin_value) = get_value(deps.branch(), conf, time, spent)? else {
            continue;
        };

        value_spent = value_spent
            .checked_add(spent_coin_value)
            .map_err(StdError::from)?;
    }

    for received in received_coins.into_iter() {
        // If the coin is not tracked (hence quoted_value = None), we don't count it towards the spending limit
        let Some(received_coin_value) = get_value(deps.branch(), conf, time, received)? else {
            continue;
        };

        value_spent = value_spent.saturating_sub(received_coin_value)
    }

    // updated value spent is only allowed to increase or stay the same
    let value_spent = max(prev_value_spent, value_spent);

    spending
        .update(value_spent, time)
        .ensure_within_limit(limit)?;

    Ok(())
}

fn get_value(
    deps: DepsMut,
    conf: &PriceResolutionConfig,
    time: Timestamp,
    coin: Coin,
) -> Result<Option<Uint128>, ContractError> {
    let Some(price_info) = get_and_cache_price(&PRICE_INFOS, deps, &conf, time, &coin.denom)?
    else {
        return Ok(None);
    };

    let value = coin
        .amount
        .checked_mul_ceil(price_info.price)
        .map_err(std_err_from_checked_mul_frac)?;

    Ok(Some(value))
}

fn std_err_from_checked_mul_frac(e: cosmwasm_std::CheckedMultiplyFractionError) -> StdError {
    match e {
        cosmwasm_std::CheckedMultiplyFractionError::DivideByZero(e) => StdError::divide_by_zero(e),
        cosmwasm_std::CheckedMultiplyFractionError::ConversionOverflow(e) => {
            StdError::ConversionOverflow { source: e }
        }
        cosmwasm_std::CheckedMultiplyFractionError::Overflow(e) => StdError::overflow(e),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::spend_limit::SpendLimitError;

    use super::*;
    use crate::price::PriceInfo;
    use cosmwasm_std::testing::{MockApi, MockQuerier};
    use cosmwasm_std::{testing::mock_dependencies, Uint64};
    use cosmwasm_std::{Decimal, MemoryStorage, OwnedDeps};

    use osmosis_std::types::osmosis::poolmanager::v1beta1::SwapAmountInRoute;
    use rstest::fixture;
    use rstest::rstest;

    #[fixture]
    fn price_resolution_config() -> PriceResolutionConfig {
        PriceResolutionConfig {
            quote_denom: "uusdc".to_string(),
            staleness_threshold: Uint64::from(3_600_000_000u64),
            twap_duration: Uint64::from(3_600_000_000u64),
        }
    }

    #[fixture]
    fn deps() -> OwnedDeps<MemoryStorage, MockApi, MockQuerier> {
        mock_dependencies()
    }

    fn setup_price_infos<'a>(deps: DepsMut) {
        let uosmo_price_info = PriceInfo {
            price: Decimal::from_str("1.5").unwrap(),
            last_updated_time: Timestamp::from_seconds(1_625_702_410),
            swap_routes: vec![SwapAmountInRoute {
                pool_id: 555,
                token_out_denom: "uusdc".to_string(),
            }],
        };

        PRICE_INFOS
            .save(deps.storage, "uosmo", &uosmo_price_info)
            .unwrap();
    }

    #[rstest]
    #[case::no_spent_no_received(vec![], vec![], 0, 0, Ok(()))]
    #[case::spent_only(vec![Coin::new(100, "uosmo")], vec![], 0, 150, Ok(()))]
    #[case::received_only_no_decrese(vec![], vec![Coin::new(100, "uosmo")], 150, 150, Ok(()))]
    #[case::received_only_no_decrese(vec![], vec![Coin::new(100, "uosmo"), Coin::new(100, "uusdc")], 150, 150, Ok(()))]
    #[case::spent_and_received(vec![Coin::new(100, "uosmo")], vec![Coin::new(50, "uusdc")], 150, 250, Ok(()))]
    #[case::untracked_coin(vec![Coin::new(100, "unknown")], vec![], 0, 0, Ok(()))]
    #[case::untracked_coin(vec![Coin::new(100, "uusdc")], vec![Coin::new(100, "unknown")], 0, 100, Ok(()))]
    #[case::exceed_spend_limit(vec![Coin::new(1_000_000, "uosmo")], vec![], 0, 1_500_000, Err(SpendLimitError::overspend(1_000_000, 1_500_000).into()))]
    #[case::at_spend_limit(vec![Coin::new(1_000_000, "uusdc")], vec![], 0, 1_000_000, Ok(()))]
    #[case::at_spend_limit(vec![Coin::new(1_000_000, "uosmo")], vec![Coin::new(500_000, "uusdc")], 0, 1_000_000, Ok(()))]
    fn test_update_and_check_spend_limit(
        mut deps: OwnedDeps<MemoryStorage, MockApi, MockQuerier>,
        price_resolution_config: PriceResolutionConfig,
        #[case] spent_coins: Vec<Coin>,
        #[case] received_coins: Vec<Coin>,
        #[case] initial_spending: u128,
        #[case] expected_spending: u128,
        #[case] expected_result: Result<(), ContractError>,
    ) {
        setup_price_infos(deps.as_mut());
        let time = Timestamp::from_seconds(1_625_702_410); // Arbitrary fixed timestamp

        let mut spending = Spending {
            value_spent_in_period: Uint128::from(initial_spending),
            last_spent_at: time.minus_seconds(5),
        };

        let limit = Uint128::from(1_000_000u128);

        let result = update_and_check_spend_limit(
            deps.as_mut(),
            &mut spending,
            spent_coins,
            received_coins,
            &price_resolution_config,
            limit,
            &Period::Day,
            time,
        );

        assert_eq!(result, expected_result);

        assert_eq!(
            spending,
            Spending {
                value_spent_in_period: Uint128::from(expected_spending),
                last_spent_at: time,
            }
        );
    }

    /// ensure that get value rounds up the multiplication result
    /// This is important because if we can spend 0.x repeatedly,
    /// it can be spent without limit as it rounds down to 0
    #[test]
    fn test_get_value_must_round_up() {
        let mut deps = mock_dependencies();
        setup_price_infos(deps.as_mut());

        let conf = PriceResolutionConfig {
            quote_denom: "uusdc".to_string(),
            staleness_threshold: Uint64::from(3_600_000_000u64),
            twap_duration: Uint64::from(3_600_000_000u64),
        };

        let time = Timestamp::from_seconds(1_625_702_410); // Arbitrary fixed timestamp

        let coin = Coin::new(1, "uosmo");

        let value = get_value(deps.as_mut(), &conf, time, coin)
            .unwrap()
            .unwrap()
            .u128();

        assert_eq!(value, 2);
    }
}
