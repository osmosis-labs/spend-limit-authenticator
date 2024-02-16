use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint64;

#[cw_serde]
pub struct PriceResolutionConfig {
    /// Denom that the price is quoted in
    pub quote_denom: String,

    /// Duration in nanoseconds that the price is considered stale.
    /// If the current time is greater than the last_updated_time + staleness_threshold,
    /// the price needs to be updated.
    pub staleness_threshold: Uint64,

    /// Twap duration in nanoseconds
    pub twap_duration: Uint64,
}
