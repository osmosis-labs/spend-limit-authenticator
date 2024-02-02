use itertools::Itertools;
use std::{cmp::Ordering, collections::HashMap};

use cosmwasm_std::{Coin, Uint128};

#[derive(Clone, Debug, PartialEq)]
pub enum DeltaType {
    Negative,
    Positive,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Delta {
    pub denom: String,
    pub ty: DeltaType,
    pub amount: Uint128,
}

impl Delta {
    pub fn new(denom: &str, ty: DeltaType, amount: Uint128) -> Self {
        Self {
            denom: denom.to_string(),
            ty,
            amount,
        }
    }

    pub fn positive(denom: &str, amount: Uint128) -> Self {
        Self::new(denom, DeltaType::Positive, amount)
    }

    pub fn negative(denom: &str, amount: Uint128) -> Self {
        Self::new(denom, DeltaType::Negative, amount)
    }
}

pub fn balances_delta(
    balances_before_spent: Vec<Coin>,
    balances_after_spent: Vec<Coin>,
) -> Vec<Delta> {
    let balances_before_spent = to_balances_map(balances_before_spent);
    let balances_after_spent = to_balances_map(balances_after_spent);

    let denoms = balances_before_spent
        .keys()
        .chain(balances_after_spent.keys())
        .unique()
        .sorted();

    let mut deltas = vec![];

    for denom in denoms {
        let amount_before = balances_before_spent
            .get(denom)
            .cloned()
            .unwrap_or_default();
        let amount_after = balances_after_spent.get(denom).cloned().unwrap_or_default();

        match amount_before.cmp(&amount_after) {
            // no change => no delta
            Ordering::Equal => continue,
            // before < after => positive delta
            Ordering::Less => deltas.push(Delta::positive(
                denom,
                amount_after.saturating_sub(amount_before),
            )),
            // before > after => negative delta
            Ordering::Greater => deltas.push(Delta::negative(
                denom,
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

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::no_delta(vec![], vec![], vec![])]
    #[case::no_delta(vec![Coin::new(100, "uosmo")], balances_before_spent.clone(), vec![])]
    #[case::no_delta(vec![Coin::new(100, "uosmo"), Coin::new(1023, "usomething")], balances_before_spent.clone(), vec![])]
    #[case::receive(
        vec![Coin::new(100, "uosmo")],
        vec![Coin::new(100, "uosmo"), Coin::new(200, "usomething")],
        vec![Delta::positive("usomething", Uint128::new(200))]
    )]
    #[case::receive(
        vec![Coin::new(100, "uosmo")],
        vec![Coin::new(101, "uosmo"), Coin::new(200, "usomething")],
        vec![
            Delta::positive("uosmo", Uint128::new(1)),
            Delta::positive("usomething", Uint128::new(200)),
        ]
    )]
    #[case::spend(
        vec![Coin::new(100, "uosmo"), Coin::new(200, "usomething")],
        vec![Coin::new(99, "uosmo"), Coin::new(200, "usomething")],
        vec![Delta::negative("uosmo", Uint128::new(1))]
    )]
    #[case::spend(
        vec![Coin::new(100, "uosmo"), Coin::new(200, "usomething")],
        vec![Coin::new(99, "uosmo"), Coin::new(199, "usomething")],
        vec![
            Delta::negative("uosmo", Uint128::new(1)),
            Delta::negative("usomething", Uint128::new(1)),
        ]
    )]
    #[case::spend_and_receive(
        vec![Coin::new(100, "uosmo"), Coin::new(200, "usomething")],
        vec![Coin::new(99, "uosmo")],
        vec![
            Delta::negative("uosmo", Uint128::new(1)),
            Delta::negative("usomething", Uint128::new(200)),
        ]
    )]
    #[case::mixed(
        vec![Coin::new(100, "uosmo"), Coin::new(200, "usomething")],
        vec![Coin::new(99, "uosmo"), Coin::new(200, "usomething"), Coin::new(100, "uother")],
        vec![
            Delta::negative("uosmo", Uint128::new(1)),
            Delta::positive("uother", Uint128::new(100)),
        ]
    )]

    pub fn test_balances_delta(
        #[case] balances_before_spent: Vec<Coin>,
        #[case] balances_after_spent: Vec<Coin>,
        #[case] expected: Vec<Delta>,
    ) {
        let deltas = balances_delta(balances_before_spent, balances_after_spent);
        assert_eq!(expected, deltas);
    }
}
