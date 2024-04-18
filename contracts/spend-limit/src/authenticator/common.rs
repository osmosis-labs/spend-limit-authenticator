use cosmwasm_std::{Addr, Coin, DepsMut, Timestamp, Uint128};

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

// TODO: calculate diff properly,
pub fn try_spend_all(
    mut deps: DepsMut,
    spending: &mut Spending,
    coins: impl IntoIterator<Item = Coin>,
    conf: &PriceResolutionConfig,
    limit: Uint128,
    reset_period: &Period,
    time: Timestamp,
) -> Result<(), ContractError> {
    for coin in coins.into_iter() {
        // If the coin is not tracked (hence get_and_cache_price = None), we don't count it towards the spending limit
        let Some(price_info) =
            get_and_cache_price(&PRICE_INFOS, deps.branch(), &conf, time, &coin.denom)?
        else {
            continue;
        };

        spending.unchecked_spend(coin.amount, price_info.price, reset_period, time)?;
    }

    spending.ensure_within_limit(limit)?;

    Ok(())
}
