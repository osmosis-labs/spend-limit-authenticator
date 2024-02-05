use std::collections::HashMap;

use cosmwasm_std::{Coin, Uint128};

/// Calculate the spendings from the pre-execution balances and the post-execution balances.
/// Ignores received coins.
pub fn calculate_spendings(
    balances_pre_exec: Vec<Coin>,
    balances_post_exec: Vec<Coin>,
) -> Vec<Coin> {
    let balances_post_exec = to_balances_map(balances_post_exec);

    // Goes through all pre-execution balances and checks if they were spent.
    // We ignore the post-execution denoms that were not present in the pre-execution denoms
    // because that means they were received, not spent
    balances_pre_exec
        .into_iter()
        .filter_map(|balance_pre_exec| {
            let amount_post_exec = balances_post_exec.get(&balance_pre_exec.denom).cloned();

            match amount_post_exec {
                // If the pre-execution denom is present in the post-execution balances,
                // we compare the amount with the pre-execution amount
                Some(amount_post_exec) => {
                    let amount_pre_exec = balance_pre_exec.amount;

                    // If post-execution amount is less than pre-execution amount, it means it was spent
                    let is_amount_decreased = amount_post_exec < amount_pre_exec;
                    if is_amount_decreased {
                        Some(Coin::new(
                            amount_pre_exec.saturating_sub(amount_post_exec).u128(),
                            &balance_pre_exec.denom,
                        ))
                    } else {
                        None
                    }
                }
                // If the balance was not present in the post-execution balances, it means all of it was spent
                None => Some(balance_pre_exec),
            }
        })
        .collect()
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
        vec![]
    )]
    #[case::receive(
        vec![Coin::new(100, "uosmo")],
        vec![Coin::new(101, "uosmo"), Coin::new(200, "usomething")],
        vec![]
    )]
    #[case::spend(
        vec![Coin::new(100, "uosmo"), Coin::new(200, "usomething")],
        vec![Coin::new(99, "uosmo"), Coin::new(200, "usomething")],
        vec![Coin::new(1, "uosmo")]
    )]
    #[case::spend(
        vec![Coin::new(100, "uosmo"), Coin::new(200, "usomething")],
        vec![Coin::new(99, "uosmo"), Coin::new(199, "usomething")],
        vec![
            Coin::new(1, "uosmo"),
            Coin::new(1, "usomething"),
        ]
    )]
    #[case::spend_and_receive(
        vec![Coin::new(100, "uosmo"), Coin::new(200, "usomething")],
        vec![Coin::new(99, "uosmo")],
        vec![
            Coin::new(1, "uosmo"),
            Coin::new(200, "usomething"),
        ]
    )]
    #[case::mixed(
        vec![Coin::new(100, "uosmo"), Coin::new(200, "usomething")],
        vec![Coin::new(99, "uosmo"), Coin::new(200, "usomething"), Coin::new(100, "uother")],
        vec![
            Coin::new(1, "uosmo"),
        ]
    )]

    pub fn test_spent_coins(
        #[case] balances_before_spent: Vec<Coin>,
        #[case] balances_after_spent: Vec<Coin>,
        #[case] expected: Vec<Coin>,
    ) {
        let deltas = calculate_spendings(balances_before_spent, balances_after_spent);
        assert_eq!(expected, deltas);
    }
}
