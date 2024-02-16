use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, Coin, Coins, Fraction, Timestamp, Uint128};

use crate::spend_limit::error::SpendLimitError;

use super::{
    error::SpendLimitResult,
    period::{to_offset_datetime, Period},
};

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

    pub fn spend(
        &mut self,
        amount: Uint128,
        price: impl Fraction<Uint128>,
        limit: Uint128,
        period: &Period,
        at: Timestamp,
    ) -> SpendLimitResult<&mut Self> {
        let spending_value = amount.multiply_ratio(price.numerator(), price.denominator());

        let value_spent_in_period = self.get_or_reset_value_spent(period, at)?;
        let updated_value_spent_in_period = value_spent_in_period.checked_add(spending_value)?;

        // ensure that the value spent in the period is not over the limit
        ensure!(
            updated_value_spent_in_period <= limit,
            SpendLimitError::Overspend {
                remaining: limit.saturating_sub(value_spent_in_period),
                requested: spending_value,
            }
        );

        self.value_spent_in_period = updated_value_spent_in_period;
        self.last_spent_at = at;

        Ok(self)
    }

    /// Get the value spent in the period.
    /// If the period has changed, the value spent in the period is reset to zero.
    fn get_or_reset_value_spent(
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
    pre_exec_balances: Coins,
    post_exec_balances: Coins,
) -> Result<Coins, SpendLimitError> {
    let mut spent_coins = Coins::default();

    // Goes through all pre-execution balances and checks if they were spent.
    // We ignore the post-execution denoms that were not present in the pre-execution denoms
    // because that means they were received, not spent
    for pre_exec_balance in pre_exec_balances.into_iter() {
        let denom = pre_exec_balance.denom;
        let pre_exec_amount = pre_exec_balance.amount;
        let post_exec_amount = post_exec_balances.amount_of(&denom);

        let spent_amount = pre_exec_amount.saturating_sub(post_exec_amount).u128();
        spent_coins.add(Coin::new(spent_amount, &denom))?;
    }

    Ok(spent_coins)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    use cosmwasm_std::Decimal;
    use time::macros::datetime;
    use time::OffsetDateTime;

    #[test]
    fn test_spending_flow() {
        // create new spending tracker
        let mut spending = Spending::default();
        let period = Period::Day;

        assert_eq!(spending.value_spent_in_period, Uint128::zero());
        assert_eq!(spending.last_spent_at, Timestamp::from_nanos(0));

        // try spending half the limit
        let limit = Uint128::from(100_000_000u128);
        let at = to_timestamp(datetime!(2024-01-01 00:00:00 UTC));
        let conversion_rate = Decimal::one();

        spending
            .spend(
                Uint128::from(50_000_000u128),
                conversion_rate,
                limit,
                &period,
                at,
            )
            .unwrap();

        assert_eq!(
            spending.value_spent_in_period,
            Uint128::from(50_000_000u128)
        );
        assert_eq!(spending.last_spent_at, at);

        // try spending a bit over the limit
        let at = to_timestamp(datetime!(2024-01-01 23:59:59 UTC));
        let err = spending
            .clone()
            .spend(
                Uint128::from(50_000_001u128),
                conversion_rate,
                limit,
                &period,
                at,
            )
            .unwrap_err();

        assert_eq!(
            err,
            SpendLimitError::Overspend {
                remaining: Uint128::from(50_000_000u128),
                requested: Uint128::from(50_000_001u128),
            }
        );

        // try spending a all the limit
        let at = to_timestamp(datetime!(2024-01-01 23:59:59 UTC));
        let spending = spending
            .spend(
                Uint128::from(50_000_000u128),
                conversion_rate,
                limit,
                &period,
                at,
            )
            .unwrap();

        assert_eq!(
            spending.value_spent_in_period,
            Uint128::from(100_000_000u128)
        );
        assert_eq!(spending.last_spent_at, at);

        // reset if new period
        let at = to_timestamp(datetime!(2024-01-02 00:00:00 UTC));
        let spending = spending
            .spend(Uint128::zero(), conversion_rate, limit, &period, at)
            .unwrap();

        assert_eq!(spending.value_spent_in_period, Uint128::zero());
        assert_eq!(spending.last_spent_at, at);
    }

    #[test]
    fn test_spending_with_value_conversion() {
        let mut spending = Spending::default();
        let conversion_rate = Decimal::from_ratio(1u128, 200_000u128);
        let period = Period::Month;

        // try spending half the limit
        let limit = Uint128::from(100_000_000u128);
        let at = to_timestamp(datetime!(2024-01-01 00:00:00 UTC));

        spending
            .spend(
                Uint128::from(50_000_000u128 * 200_000u128),
                conversion_rate,
                limit,
                &period,
                at,
            )
            .unwrap();

        assert_eq!(
            spending.value_spent_in_period,
            Uint128::from(50_000_000u128)
        );
        assert_eq!(spending.last_spent_at, at);
    }

    fn to_timestamp(offset_datetime: OffsetDateTime) -> Timestamp {
        Timestamp::from_nanos(offset_datetime.unix_timestamp_nanos() as u64)
    }

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

    pub fn test_calculate_spent_coins(
        #[case] balances_before_spent: Vec<Coin>,
        #[case] balances_after_spent: Vec<Coin>,
        #[case] expected: Vec<Coin>,
    ) {
        let balances_before_spent = Coins::try_from(balances_before_spent).unwrap();
        let balances_after_spent = Coins::try_from(balances_after_spent).unwrap();
        let deltas = calculate_spent_coins(balances_before_spent, balances_after_spent).unwrap();
        let expected = Coins::try_from(expected).unwrap();
        assert_eq!(expected, deltas);
    }
}
