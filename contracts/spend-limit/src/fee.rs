use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Coin, Coins, Timestamp};

use crate::{
    period::{to_offset_datetime, Period},
    ContractError,
};

#[cw_serde]
pub struct UntrackedSpentFee {
    pub fee: Vec<Coin>,
    pub updated_at: Timestamp,
}

impl UntrackedSpentFee {
    pub fn new(at: Timestamp) -> Self {
        Self {
            fee: vec![],
            updated_at: at,
        }
    }

    pub fn accum(self, fee: Vec<Coin>, at: Timestamp) -> Result<Self, ContractError> {
        let mut acc = Coins::try_from(self.fee)?;
        for f in fee {
            acc.add(f)?;
        }

        Ok(Self {
            fee: acc.to_vec(),
            updated_at: at,
        })
    }

    pub fn get_or_reset_accum_fee(
        self,
        period: Period,
        at: Timestamp,
    ) -> Result<Vec<Coin>, ContractError> {
        let previous = to_offset_datetime(&self.updated_at)?;
        let current = to_offset_datetime(&at)?;

        if period.has_changed(previous, current)? {
            Ok(vec![])
        } else {
            Ok(self.fee)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::{coins, Timestamp};
    use rstest::*;

    #[fixture]
    fn untracked_spent_fee_empty() -> UntrackedSpentFee {
        UntrackedSpentFee::new(Timestamp::from_seconds(0))
    }

    #[fixture]
    fn untracked_spent_fee_with_fee() -> UntrackedSpentFee {
        let mut fee = UntrackedSpentFee::new(Timestamp::from_seconds(0));
        let update_fee = coins(100, "token");
        fee = fee.accum(update_fee, Timestamp::from_seconds(10)).unwrap();
        fee
    }

    #[rstest]
    fn new_untracked_spent_fee_should_have_empty_fee(untracked_spent_fee_empty: UntrackedSpentFee) {
        assert!(untracked_spent_fee_empty.fee.is_empty());
    }

    #[rstest]
    fn accum_should_add_fee_correctly(untracked_spent_fee_empty: UntrackedSpentFee) {
        let update_fee = coins(100, "token");
        let updated_fee = untracked_spent_fee_empty
            .accum(update_fee.clone(), Timestamp::from_seconds(10))
            .unwrap();
        assert_eq!(updated_fee.fee, update_fee);

        let update_fee = coins(200, "token");
        let updated_fee = updated_fee
            .accum(update_fee.clone(), Timestamp::from_seconds(20))
            .unwrap();
        assert_eq!(updated_fee.fee, coins(300, "token"));

        let update_fee = coins(100, "another_token");
        let updated_fee = updated_fee
            .accum(update_fee.clone(), Timestamp::from_seconds(30))
            .unwrap();
        assert_eq!(
            updated_fee.fee,
            vec![Coin::new(100, "another_token"), Coin::new(300, "token")]
        );
    }

    #[rstest]
    fn get_or_reset_accum_fee_should_reset_fee_after_period_change(
        untracked_spent_fee_with_fee: UntrackedSpentFee,
    ) {
        let period = Period::Day;
        let updated_fee = untracked_spent_fee_with_fee
            .get_or_reset_accum_fee(period, Timestamp::from_seconds(86401))
            .unwrap();
        assert!(updated_fee.is_empty());
    }

    #[rstest]
    fn get_or_reset_accum_fee_should_not_reset_fee_within_same_period(
        untracked_spent_fee_with_fee: UntrackedSpentFee,
    ) {
        let period = Period::Day;
        let updated_fee = untracked_spent_fee_with_fee
            .get_or_reset_accum_fee(period, Timestamp::from_seconds(3599))
            .unwrap();
        assert!(!updated_fee.is_empty());
    }
}
