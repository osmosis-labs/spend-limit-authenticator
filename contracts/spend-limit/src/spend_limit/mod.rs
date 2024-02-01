mod balance;
mod error;
mod period;
mod price;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, Coin, Fraction};
use error::SpendLimitError;
use period::Period;
use price::ValueStrategy;

#[cw_serde]
pub struct DeprecatedSpendLimit {
    pub id: String,
    pub denom: String,
    pub balance: Vec<Coin>,
    pub amount_left: u128,
    pub block_of_last_tx: u64,
    pub number_of_blocks_active: u64,
}

// -------------------------------------------

use cosmwasm_std::{Addr, Timestamp, Uint128};
use cw_storage_plus::Map;

use self::{error::SpendLimitResult, period::to_offset_datetime};

pub type SpendingStorage<'a> = Map<'a, SpendingKey<'a>, Spending>;

/// SpendingKey is a key for the spending storage.
/// It is a tuple of (account, subkey) which
/// allows multiple spend limits per account.
pub struct SpendingKey<'a>(&'a Addr, &'a str);

impl<'a> SpendingKey<'a> {
    pub fn new(addr: &'a Addr, subkey: &'a str) -> Self {
        Self(addr, subkey)
    }

    pub fn account(&self) -> &Addr {
        self.0
    }

    pub fn subkey(&self) -> &str {
        self.1
    }
}

/// State for tracking spend limit.
#[cw_serde]
#[derive(Default)]
pub struct Spending {
    /// Used for tracking the balances of the account
    /// before executing the tx.
    pub balances_before_spent: Vec<Coin>,

    /// The value spent in the current period
    /// This is reset when the period changes
    pub value_spent_in_period: Uint128,

    /// The last time the account spent
    /// This is used to check if we are in a new period
    pub last_spent_at: Timestamp,
}

#[cw_serde]
pub struct SpendLimitParams {
    /// Subkey for the account, to allow multiple spend limits per account
    subkey: String,

    /// Limit per period, used to enforce spend limit with this given amount and denom,
    /// calculated based on value strategy.
    limit: Coin,

    /// Period to reset spend limit quota
    reset_period: Period,

    /// Strategy to calculate the value of the coin spent.
    value_strategy: ValueStrategy,
}

impl Spending {
    pub fn new(last_spent: Timestamp) -> Self {
        Self {
            balances_before_spent: vec![],
            value_spent_in_period: Uint128::zero(),
            last_spent_at: last_spent,
        }
    }

    // TODO: rename to update and add enum (Spend, Receive)
    pub fn spend(
        self,
        amount: Uint128,
        conversion_rate: impl Fraction<Uint128>,
        limit: Uint128,
        period: &Period,
        at: Timestamp,
    ) -> SpendLimitResult<Self> {
        let spending_value =
            amount.multiply_ratio(conversion_rate.numerator(), conversion_rate.denominator());

        let value_spent_in_period = self
            .get_or_reset_value_spent(period, at)?
            .checked_add(spending_value)?;

        // ensure that the value spent in the period is not over the limit
        ensure!(
            value_spent_in_period <= limit,
            SpendLimitError::OverSpent {
                remaining: limit.saturating_sub(value_spent_in_period),
                requested: spending_value,
            }
        );

        Ok(Self {
            value_spent_in_period,
            last_spent_at: at,
            ..self
        })
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

#[cfg(test)]
mod tests {
    use cosmwasm_std::Decimal;
    use time::macros::datetime;
    use time::OffsetDateTime;

    use super::*;

    #[test]
    fn test_spending_flow() {
        // create new spending tracker
        let spending = Spending::default();
        let period = Period::Day;

        assert_eq!(spending.value_spent_in_period, Uint128::zero());
        assert_eq!(spending.last_spent_at, Timestamp::from_nanos(0));

        // try spending half the limit
        let limit = Uint128::from(100_000_000u128);
        let at = to_timestamp(datetime!(2024-01-01 00:00:00 UTC));
        let conversion_rate = Decimal::one();

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
            SpendLimitError::OverSpent {
                remaining: Uint128::zero(),
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
        let spending = Spending::default();
        let conversion_rate = Decimal::from_ratio(1u128, 200_000u128);
        let period = Period::Month;

        // try spending half the limit
        let limit = Uint128::from(100_000_000u128);
        let at = to_timestamp(datetime!(2024-01-01 00:00:00 UTC));

        let spending = spending
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
}
