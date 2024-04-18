use std::cmp::max;

use cosmwasm_std::{Addr, Coin, DepsMut, Fraction, StdError, Timestamp, Uint128};

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

        value_spent = value_spent
            .checked_sub(received_coin_value)
            .map_err(StdError::from)?;
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

    Ok(Some(coin.amount.multiply_ratio(
        price_info.price.numerator(),
        price_info.price.denominator(),
    )))
}
