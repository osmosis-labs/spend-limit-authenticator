use cosmwasm_schema::{cw_serde, QueryResponses};
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
pub struct InstantiateMsg {
    pub price_resolution_config: PriceResolutionConfig,
    pub tracked_denoms: Vec<TrackedDenom>,
    pub admin: Option<String>,
}

#[cw_serde]
pub enum ExecuteMsg {
    TransferAdmin { address: String },
    ClaimAdmin {},
    RevokeAdmin {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
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
