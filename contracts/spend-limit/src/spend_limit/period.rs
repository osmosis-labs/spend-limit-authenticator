use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, Timestamp};
use thiserror::Error;
use time::OffsetDateTime;

/// Period of time for spend limit.
/// Note that week is Monday-based.
#[cw_serde]
pub enum Period {
    Day,
    Week,
    Month,
    Year,
}

#[derive(Error, Debug, PartialEq)]
pub enum PeriodError {
    #[error("Timestamp conversion cause component out of range: {0}")]
    TimestampConversionOutOfRange(time::error::ComponentRange),

    #[error("Previous time must be before, or the same as current time: previous {previous}, current {current}")]
    InvalidTimeComparison {
        previous: OffsetDateTime,
        current: OffsetDateTime,
    },
}

type PeriodResult<T> = Result<T, PeriodError>;

/// Check whether the current time has entered new [`Period`]
/// compared to the previous time.
pub fn has_entered_new_period(
    period: Period,
    previous: OffsetDateTime,
    current: OffsetDateTime,
) -> PeriodResult<bool> {
    // if the current time is the same as the previous time,
    // it means that the tx is executed in the same block.
    // In that case, we can assume that the tx is not executed in the new period.
    if previous == current {
        return Ok(false);
    }

    // ensure previous time is before or the same as  current time
    ensure!(
        previous <= current,
        PeriodError::InvalidTimeComparison { previous, current }
    );

    // otherwise, check whether the current time has entered new period
    // based on the period type.
    Ok(match period {
        Period::Day => previous.date() < current.date(),
        Period::Week => {
            let prev_spent_at_week = previous.monday_based_week();
            let current_week = current.monday_based_week();

            let has_entered_new_year = previous.year() < current.year();
            let is_same_year_but_entered_new_week =
                previous.year() == current.year() && prev_spent_at_week < current_week;

            has_entered_new_year || is_same_year_but_entered_new_week
        }
        Period::Month => {
            let prev_spent_at_month = previous.month() as u8;
            let current_month = current.month() as u8;

            let has_entered_new_year = previous.year() < current.year();
            let is_same_year_but_entered_new_month =
                previous.year() == current.year() && prev_spent_at_month < current_month;

            has_entered_new_year || is_same_year_but_entered_new_month
        }
        Period::Year => previous.year() < current.year(),
    })
}

/// Convert Timestamp to OffsetDateTime.
pub fn to_offset_datetime(timestamp: &Timestamp) -> PeriodResult<OffsetDateTime> {
    OffsetDateTime::from_unix_timestamp_nanos(timestamp.nanos() as i128)
        .map_err(PeriodError::TimestampConversionOutOfRange)
}

#[cfg(test)]
mod tests {

    use super::*;
    use rstest::rstest;
    use time::macros::datetime;

    #[rstest]
    // same time
    #[case(Period::Day, datetime!(2023-01-01 0:00:00 UTC), datetime!(2023-01-01 00:00:00 UTC), Ok(false))]
    #[case(Period::Day, datetime!(2025-11-01 0:00:00 UTC), datetime!(2025-11-01 00:00:00 UTC), Ok(false))]
    // day
    #[case(Period::Day, datetime!(2023-01-02 0:01:00 UTC), datetime!(2023-04-10 12:01:10 UTC), Ok(true))]
    #[case(Period::Day, datetime!(2023-01-01 0:00:00 UTC), datetime!(2023-01-02 00:00:00 UTC), Ok(true))]
    #[case(Period::Day, datetime!(2023-01-02 0:00:00 UTC), datetime!(2023-01-02 00:00:01 UTC), Ok(false))]
    #[case(Period::Day, datetime!(2026-01-02 0:01:00 UTC), datetime!(2026-01-02 00:01:02 UTC), Ok(false))]
    // week
    #[case(Period::Week, datetime!(2024-01-01 0:00:00 UTC), datetime!(2024-01-08 00:00:00 UTC), Ok(true))]
    #[case(Period::Week, datetime!(2024-01-01 0:00:00 UTC), datetime!(2025-01-01 00:00:00 UTC), Ok(true))]
    #[case(Period::Week, datetime!(2022-03-09 0:05:00 UTC), datetime!(2025-03-14 00:00:00 UTC), Ok(true))]
    #[case(Period::Week, datetime!(2024-01-01 0:00:00 UTC), datetime!(2024-02-01 00:00:00 UTC), Ok(true))]
    #[case(Period::Week, datetime!(2024-01-01 0:00:00 UTC), datetime!(2025-01-02 00:00:00 UTC), Ok(true))]
    #[case(Period::Week, datetime!(2024-01-01 0:00:00 UTC), datetime!(2024-01-07 23:59:59 UTC), Ok(false))]
    #[case(Period::Week, datetime!(2024-01-01 0:00:00 UTC), datetime!(2024-01-01 00:00:00 UTC), Ok(false))]
    // month
    #[case(Period::Month, datetime!(2022-01-01 0:00:00 UTC), datetime!(2022-02-01 00:00:00 UTC), Ok(true))]
    #[case(Period::Month, datetime!(2022-01-02 0:01:00 UTC), datetime!(2022-02-02 00:01:02 UTC), Ok(true))]
    #[case(Period::Month, datetime!(2022-01-01 0:00:00 UTC), datetime!(2022-02-01 00:00:00 UTC), Ok(true))]
    #[case(Period::Month, datetime!(2022-01-01 0:00:00 UTC), datetime!(2023-01-01 00:00:00 UTC), Ok(true))]
    #[case(Period::Month, datetime!(2022-01-01 0:00:00 UTC), datetime!(2022-01-31 23:59:59 UTC), Ok(false))]
    // year
    #[case(Period::Year, datetime!(2022-01-01 0:00:00 UTC), datetime!(2023-01-01 00:00:00 UTC), Ok(true))]
    #[case(Period::Year, datetime!(2022-01-02 0:01:00 UTC), datetime!(2023-01-02 00:01:02 UTC), Ok(true))]
    #[case(Period::Year, datetime!(2022-01-02 0:01:00 UTC), datetime!(2022-12-31 23:59:59 UTC), Ok(false))]
    // current < previous
    #[case(Period::Day, datetime!(2022-01-01 0:00:00 UTC), datetime!(2021-12-31 23:59:59 UTC), Err(PeriodError::InvalidTimeComparison { previous, current }))]
    #[case(Period::Day, datetime!(2022-05-07 0:00:00 UTC), datetime!(2022-05-06 23:59:59 UTC), Err(PeriodError::InvalidTimeComparison { previous, current }))]

    fn test_has_entered_new_period(
        #[case] period: Period,
        #[case] previous: OffsetDateTime,
        #[case] current: OffsetDateTime,
        #[case] expected: PeriodResult<bool>,
    ) {
        assert_eq!(has_entered_new_period(period, previous, current), expected);
    }

    #[rstest]
    #[case(0, datetime!(1970-01-01 00:00:00 UTC))]
    #[case(1706756691000000000, datetime!(2024-02-01 03:04:51 UTC))]
    #[case(u64::MAX, datetime!(2554-07-21 23:34:33.709551615 UTC))]
    fn test_to_offset_datetime(#[case] nanos_since_epoch: u64, #[case] expected: OffsetDateTime) {
        assert_eq!(
            Ok(expected),
            to_offset_datetime(&Timestamp::from_nanos(nanos_since_epoch)),
        );
    }
}
