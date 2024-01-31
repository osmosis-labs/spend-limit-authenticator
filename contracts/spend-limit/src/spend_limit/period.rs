use cosmwasm_schema::cw_serde;
use time::PrimitiveDateTime;

#[cw_serde]
pub enum Period {
    Day,
    Week,
    Month,
    Year,
}

// TODO: consider error when current < prev_spent_at
fn is_period_over(
    period: Period,
    current: impl Into<PrimitiveDateTime>,
    prev_spent_at: impl Into<PrimitiveDateTime>,
) -> bool {
    let current = current.into();
    let prev_spent_at = prev_spent_at.into();

    match period {
        Period::Day => current.date() > prev_spent_at.date(),
        Period::Week => {
            let current_week = current.monday_based_week();
            let prev_spent_at_week = prev_spent_at.monday_based_week();
            current.year() >= prev_spent_at.year() && current_week > prev_spent_at_week
        }
        Period::Month => {
            let current_month: u8 = current.month().into();
            let prev_spent_at_month: u8 = prev_spent_at.month().into();
            current.year() >= prev_spent_at.year() && current_month > prev_spent_at_month
        }
        Period::Year => current.year() > prev_spent_at.year(),
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use rstest::rstest;
    use time::macros::datetime;

    #[rstest]
    // day
    #[case(Period::Day, datetime!(2023-04-10 12:01:10), datetime!(2023-01-02 0:01:00), true)]
    #[case(Period::Day, datetime!(2023-01-02 0:00:00), datetime!(2023-01-01 0:00:00), true)]
    #[case(Period::Day, datetime!(2023-01-02 0:00:01), datetime!(2023-01-02 0:00:00), false)]
    #[case(Period::Day, datetime!(2023-01-02 0:01:02), datetime!(2023-01-02 0:01:00), false)]
    // week
    // TODO: add tests
    // month
    // TODO: add tests
    // year
    #[case(Period::Year, datetime!(2023-01-01 0:00:00), datetime!(2022-01-01 0:00:00), true)]
    #[case(Period::Year, datetime!(2023-01-02 0:01:02), datetime!(2022-01-02 0:01:00), true)]
    #[case(Period::Year, datetime!(2022-12-31 23:59:59), datetime!(2022-01-02 0:01:00), false)]
    fn test_is_period_over(
        #[case] period: Period,
        #[case] current: PrimitiveDateTime,
        #[case] prev_spent_at: PrimitiveDateTime,
        #[case] expected: bool,
    ) {
        assert_eq!(is_period_over(period, current, prev_spent_at), expected);
    }
}
