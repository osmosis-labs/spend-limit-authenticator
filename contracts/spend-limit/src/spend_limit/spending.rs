use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Coin, Coins, Timestamp, Uint128};

use crate::{
    period::{to_offset_datetime, Period},
    spend_limit::error::SpendLimitError,
};

use super::error::SpendLimitResult;

/// State for tracking spend limit.
#[cw_serde]
#[derive(Default)]
pub struct Spending {
    /// The value spent in the current period
    /// This is reset when the period changes
    pub value_spent_in_period: Uint128,

    /// The last time the account spent
    /// This is used to check if we are in a new period
    pub last_spent_at: Timestamp,
}

impl Spending {
    pub fn new(last_spent: Timestamp) -> Self {
        Self {
            value_spent_in_period: Uint128::zero(),
            last_spent_at: last_spent,
        }
    }

    pub fn update(
        &mut self,
        value_spent_in_period: Uint128,
        last_spent_at: Timestamp,
    ) -> &mut Self {
        self.value_spent_in_period = value_spent_in_period;
        self.last_spent_at = last_spent_at;

        self
    }

    /// ensure that the value spent in the period is not over the limit
    pub fn ensure_within_limit(&self, limit: Uint128) -> SpendLimitResult<()> {
        if self.value_spent_in_period > limit {
            Err(SpendLimitError::Overspend {
                limit,
                spent: self.value_spent_in_period,
            })
        } else {
            Ok(())
        }
    }

    /// Get the value spent in the period.
    /// If the period has changed, the value spent in the period is reset to zero.
    pub fn get_or_reset_value_spent(
        &self,
        period: &Period,
        at: Timestamp,
    ) -> SpendLimitResult<Uint128> {
        let previous = to_offset_datetime(&self.last_spent_at)?;
        let current = to_offset_datetime(&at)?;

        if period.has_changed(previous, current)? {
            Ok(Uint128::zero())
        } else {
            Ok(self.value_spent_in_period)
        }
    }
}

/// Calculate the spendings from the pre-execution balances and the post-execution balances.
/// Ignores received coins.
pub fn calculate_spent_coins(
    pre_exec_balances: &Coins,
    post_exec_balances: &Coins,
) -> Result<Coins, SpendLimitError> {
    let mut spent_coins = Coins::default();

    // Goes through all pre-execution balances and checks if they were spent.
    // We ignore the post-execution denoms that were not present in the pre-execution denoms
    // because that means they were received, not spent
    for pre_exec_balance in pre_exec_balances.into_iter() {
        let denom = &pre_exec_balance.denom;
        let pre_exec_amount = pre_exec_balance.amount;
        let post_exec_amount = post_exec_balances.amount_of(denom);

        let spent_amount = pre_exec_amount.saturating_sub(post_exec_amount).u128();
        spent_coins.add(Coin::new(spent_amount, denom))?;
    }

    Ok(spent_coins)
}

// Calculate the received coins from the pre-execution balances and the post-execution balances.
// Ignores spent coins.
pub fn calculate_received_coins(
    pre_exec_balances: &Coins,
    post_exec_balances: &Coins,
) -> Result<Coins, SpendLimitError> {
    let mut received_coins = Coins::default();

    // Goes through all post-execution balances and checks if they were received.
    // We ignore the pre-execution denoms that were not present in the post-execution denoms
    // because that means they were spent, not received
    for post_exec_balance in post_exec_balances.into_iter() {
        let denom = &post_exec_balance.denom;
        let post_exec_amount = post_exec_balance.amount;
        let pre_exec_amount = pre_exec_balances.amount_of(denom);

        let received_amount = post_exec_amount.saturating_sub(pre_exec_amount).u128();
        received_coins.add(Coin::new(received_amount, denom))?;
    }

    Ok(received_coins)
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
    #[case::spend(
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

    pub fn test_calculate_spent_coins(
        #[case] balances_before_spent: Vec<Coin>,
        #[case] balances_after_spent: Vec<Coin>,
        #[case] expected: Vec<Coin>,
    ) {
        let balances_before_spent = Coins::try_from(balances_before_spent).unwrap();
        let balances_after_spent = Coins::try_from(balances_after_spent).unwrap();
        let deltas = calculate_spent_coins(&balances_before_spent, &balances_after_spent).unwrap();
        let expected = Coins::try_from(expected).unwrap();
        assert_eq!(expected, deltas);
    }

    #[rstest]
    #[case::no_delta(vec![], vec![], vec![])]
    #[case::no_delta(vec![Coin::new(100, "uosmo")], balances_before_spent.clone(), vec![])]
    #[case::no_delta(vec![Coin::new(100, "uosmo"), Coin::new(1023, "usomething")], balances_before_spent.clone(), vec![])]
    #[case::receive(
        vec![Coin::new(100, "uosmo")],
        vec![Coin::new(100, "uosmo"), Coin::new(200, "usomething")],
        vec![Coin::new(200, "usomething")]
    )]
    #[case::receive(
        vec![Coin::new(100, "uosmo")],
        vec![Coin::new(101, "uosmo"), Coin::new(200, "usomething")],
        vec![Coin::new(1, "uosmo"), Coin::new(200, "usomething")]
    )]
    #[case::spend(
        vec![Coin::new(100, "uosmo"), Coin::new(200, "usomething")],
        vec![Coin::new(99, "uosmo"), Coin::new(200, "usomething")],
        vec![]
    )]
    #[case::spend(
        vec![Coin::new(100, "uosmo"), Coin::new(200, "usomething")],
        vec![Coin::new(99, "uosmo"), Coin::new(199, "usomething")],
        vec![]
    )]
    #[case::spend(
        vec![Coin::new(100, "uosmo"), Coin::new(200, "usomething")],
        vec![Coin::new(99, "uosmo")],
        vec![]
    )]
    #[case::mixed(
        vec![Coin::new(100, "uosmo"), Coin::new(200, "usomething")],
        vec![Coin::new(99, "uosmo"), Coin::new(200, "usomething"), Coin::new(100, "uother")],
        vec![
            Coin::new(100, "uother"),
        ]
    )]
    pub fn test_calculate_received_coins(
        #[case] balances_before_spent: Vec<Coin>,
        #[case] balances_after_spent: Vec<Coin>,
        #[case] expected: Vec<Coin>,
    ) {
        let balances_before_spent = Coins::try_from(balances_before_spent).unwrap();
        let balances_after_spent = Coins::try_from(balances_after_spent).unwrap();
        let deltas =
            calculate_received_coins(&balances_before_spent, &balances_after_spent).unwrap();
        let expected = Coins::try_from(expected).unwrap();
        assert_eq!(expected, deltas);
    }
}
