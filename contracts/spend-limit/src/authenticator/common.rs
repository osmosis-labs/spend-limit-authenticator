use cosmwasm_std::{Addr, Coin, DepsMut, Timestamp, Uint128};

use crate::{
    period::Period,
    price::{get_and_cache_price, PriceResolutionConfig},
    spend_limit::Spending,
    state::PRICE_INFOS,
    ContractError,
};

pub fn get_account_spending_fee(
    account: &Addr,
    fee_payer: &Addr,
    fee_granter: Option<&Addr>,
    fee: Vec<Coin>,
) -> Vec<Coin> {
    if let Some(fee_granter) = fee_granter {
        if account == fee_granter {
            fee
        } else {
            vec![]
        }
    } else {
        if account == fee_payer {
            fee
        } else {
            vec![]
        }
    }
}

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

        spending.try_spend(
            coin.amount,
            price_info.price,
            limit.into(),
            reset_period,
            time,
        )?;
    }

    Ok(())
}