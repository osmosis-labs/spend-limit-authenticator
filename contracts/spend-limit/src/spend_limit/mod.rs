mod error;
mod params;
mod spending;

use crate::{
    authenticator::{
        AuthenticatorError, CompositeAuthenticator, CompositeId, CosmwasmAuthenticatorData,
    },
    fee::UntrackedSpentFeeStore,
    period::Period,
    price::{get_and_cache_price, get_price, PriceInfoStore, PriceResolutionConfig},
    ContractError,
};
use cosmwasm_std::{from_json, Coin, Deps};
use cosmwasm_std::{DepsMut, StdError, Timestamp, Uint128};
pub use error::SpendLimitError;
use osmosis_std::types::osmosis::smartaccount::v1beta1::SmartaccountQuerier;
pub use params::{SpendLimitParams, TimeLimit};
pub use spending::{calculate_received_coins, calculate_spent_coins, Spending};
use std::{cmp::max, str::FromStr};

use cosmwasm_std::Addr;
use cw_storage_plus::Map;

pub type SpendingStore<'a> = Map<'a, SpendingKey<'a>, Spending>;

/// [`PreExecBalance`] is a map of spending keys to the coins spent.
pub type PreExecBalance<'a> = Map<'a, SpendingKey<'a>, Vec<Coin>>;

/// SpendingKey is a key for the spending storage.
/// It is a tuple of (account, authenticator_id) which
/// allows multiple spend limits per account.
pub type SpendingKey<'a> = (&'a Addr, &'a str);

pub fn update_and_check_spend_limit(
    mut deps: DepsMut,
    price_info_store: &PriceInfoStore,
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
        let Some(spent_coin_value) = get_value(deps.branch(), price_info_store, conf, time, spent)?
        else {
            continue;
        };

        value_spent = value_spent
            .checked_add(spent_coin_value)
            .map_err(StdError::from)?;
    }

    for received in received_coins.into_iter() {
        // If the coin is not tracked (hence quoted_value = None), we don't count it towards the spending limit
        let Some(received_coin_value) =
            get_value(deps.branch(), price_info_store, conf, time, received)?
        else {
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
    price_info_store: &PriceInfoStore,
    conf: &PriceResolutionConfig,
    time: Timestamp,
    coin: Coin,
) -> Result<Option<Uint128>, ContractError> {
    let Some(price_info) = get_and_cache_price(price_info_store, deps, &conf, time, &coin.denom)?
    else {
        return Ok(None);
    };

    let value = coin
        .amount
        .checked_mul_ceil(price_info.price)
        .map_err(std_err_from_checked_mul_frac)?;

    Ok(Some(value))
}

/// Get spend limit params from the authenticator data
/// This supports getting params from composite authenticator
pub fn get_spend_limit_params(
    deps: Deps,
    account: &Addr,
    authenticator_id: &str,
) -> Result<SpendLimitParams, ContractError> {
    let smart_account_querier = SmartaccountQuerier::new(&deps.querier);

    let composite_id =
        CompositeId::from_str(&authenticator_id).map_err(AuthenticatorError::from)?;

    let response =
        smart_account_querier.get_authenticator(account.to_string(), composite_id.root)?;

    let spend_limit_auth_data = response
        .account_authenticator
        .ok_or(StdError::not_found(&format!(
            "Authenticator with account = {}, authenticator_id = {}",
            account, authenticator_id
        )))?
        .child_authenticator_data::<CosmwasmAuthenticatorData>(&composite_id.path)
        .map_err(AuthenticatorError::from)?;

    from_json::<SpendLimitParams>(&spend_limit_auth_data.params).map_err(ContractError::from)
}

/// Update stored spending with updated information such as reset period, untracked spent fee
pub fn updated_spending(
    deps: Deps,
    price_info_store: &PriceInfoStore,
    untracked_spent_fee_store: &UntrackedSpentFeeStore,
    conf: &PriceResolutionConfig,
    account: &Addr,
    authenticator_id: &str,
    at: Timestamp,
    spending: Spending,
) -> Result<Spending, ContractError> {
    let params = get_spend_limit_params(deps, &account, &authenticator_id)?;
    let mut value_spent_in_period = spending.get_or_reset_value_spent(&params.reset_period, at)?;

    // add untracked spent fee as part of value spent
    let untracked_spent_fee = untracked_spent_fee_store
        .may_load(deps.storage, (account, authenticator_id))?
        .unwrap_or_default();

    let last_spent_at = spending.last_spent_at.max(untracked_spent_fee.updated_at);

    let accumulated_fee = untracked_spent_fee.get_or_reset_accum_fee(&params.reset_period, at)?;

    for fee in accumulated_fee {
        if let Some(price) = get_price(price_info_store, deps, conf, at, &fee.denom)? {
            let fee_spent = fee
                .amount
                .checked_mul_ceil(price.price)
                .map_err(std_err_from_checked_mul_frac)?;

            value_spent_in_period = value_spent_in_period
                .checked_add(fee_spent)
                .map_err(StdError::overflow)?;
        };
    }

    Ok(Spending {
        value_spent_in_period,
        last_spent_at,
    })
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
    use crate::state::PRICE_INFOS;

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
            &PRICE_INFOS,
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

        let value = get_value(deps.as_mut(), &PRICE_INFOS, &conf, time, coin)
            .unwrap()
            .unwrap()
            .u128();

        assert_eq!(value, 2);
    }
}
