use std::collections::{HashMap, HashSet};

use cosmwasm_std::{Coin, Uint128};

pub enum Direction {
    Spend,
    Receive,
}

// TODO: write tests
pub fn balances_delta(
    balances_before_spent: Vec<Coin>,
    balances_after_spent: Vec<Coin>,
) -> Vec<(String, Direction, Uint128)> {
    let balances_before_spent = to_balances_map(balances_before_spent);
    let balances_after_spent = to_balances_map(balances_after_spent);

    let denoms = balances_before_spent
        .keys()
        .chain(balances_after_spent.keys())
        .collect::<HashSet<_>>();

    let mut deltas = vec![];

    for denom in denoms {
        let amount_before = balances_before_spent
            .get(denom)
            .cloned()
            .unwrap_or_default();
        let amount_after = balances_after_spent.get(denom).cloned().unwrap_or_default();

        match amount_before.cmp(&amount_after) {
            // receive
            std::cmp::Ordering::Less => deltas.push((
                denom.clone(),
                Direction::Receive,
                amount_after.saturating_sub(amount_before),
            )),
            // no delta
            std::cmp::Ordering::Equal => continue,
            // spend
            std::cmp::Ordering::Greater => deltas.push((
                denom.clone(),
                Direction::Spend,
                amount_before.saturating_sub(amount_after),
            )),
        }
    }

    deltas
}

fn to_balances_map(balances: Vec<Coin>) -> HashMap<String, Uint128> {
    balances
        .into_iter()
        .map(|coin| (coin.denom, coin.amount))
        .collect()
}
