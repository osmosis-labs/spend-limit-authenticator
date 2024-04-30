use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint64;
pub use osmosis_std::types::osmosis::poolmanager::v1beta1::SwapAmountInRoute;

use crate::{price::PriceResolutionConfig, spend_limit::Spending};

// re-export the structs from osmosis_authenticators
pub use osmosis_authenticators::AuthenticatorSudoMsg as SudoMsg;

#[cw_serde]
pub struct TrackedDenom {
    pub denom: String,
    pub swap_routes: Vec<SwapAmountInRoute>,
}

#[cw_serde]
pub enum DenomRemovalTarget {
    All,
    Partial(Vec<String>),
}

#[cw_serde]
pub struct InstantiateMsg {
    pub price_resolution_config: PriceResolutionConfig,
    pub tracked_denoms: Vec<TrackedDenom>,
    pub admin: Option<String>,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Set the price resolution parameters
    SetPriceResolutionParams {
        /// Duration in nanoseconds that the price is considered stale.
        /// If the current time is greater than the last_updated_time + staleness_threshold,
        /// the price needs to be updated.
        staleness_threshold: Uint64,

        /// Twap duration in nanoseconds
        twap_duration: Uint64,
    },
    RemoveTrackedDenoms {
        target: DenomRemovalTarget,
    },
    /// Set tracked denoms, this will overwrite the current tracked denoms if exists
    /// or add new tracked denoms if not exists
    SetTrackedDenoms {
        tracked_denoms: Vec<TrackedDenom>,
    },
    TransferAdmin {
        address: String,
    },
    ClaimAdminTransfer {},
    RejectAdminTransfer {},
    CancelAdminTransfer {},
    RevokeAdmin {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(PriceResolutionConfigResponse)]
    PriceResolutionConfig {},

    #[returns(TrackedDenomsResponse)]
    TrackedDenoms {
        start_after: Option<String>,
        limit: Option<u32>,
    },

    #[returns(SpendingResponse)]
    Spending {
        account: String,
        authenticator_id: String,
    },

    #[returns(SpendingsByAccountResponse)]
    SpendingsByAccount { account: String },

    #[returns(AdminResponse)]
    Admin {},

    #[returns(AdminCandidateResponse)]
    AdminCandidate {},
}

#[cw_serde]
pub struct PriceResolutionConfigResponse {
    pub price_resolution_config: PriceResolutionConfig,
}

#[cw_serde]
pub struct TrackedDenomsResponse {
    pub tracked_denoms: Vec<TrackedDenom>,
}

#[cw_serde]
pub struct SpendingResponse {
    pub spending: Spending,
}

#[cw_serde]
pub struct SpendingsByAccountResponse {
    pub spendings: Vec<(String, Spending)>,
}

#[cw_serde]
pub struct AdminResponse {
    pub admin: Option<String>,
}

#[cw_serde]
pub struct AdminCandidateResponse {
    pub candidate: Option<String>,
}
