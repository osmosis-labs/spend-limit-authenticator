use cosmwasm_schema::{cw_serde, QueryResponses};
use osmosis_std::types::osmosis::poolmanager::v1beta1::SwapAmountInRoute;

use crate::{price::PriceResolutionConfig, spend_limit::Spending};

#[cw_serde]
pub struct TrackedDenom {
    pub denom: String,
    pub swap_routes: Vec<SwapAmountInRoute>,
}

#[cw_serde]
pub struct InstantiateMsg {
    pub price_resolution_config: PriceResolutionConfig,
    pub tracked_denoms: Vec<TrackedDenom>,
}

#[cw_serde]
pub enum SudoMsg {
    OnAuthenticatorAdded(osmosis_authenticators::OnAuthenticatorAddedRequest),
    OnAuthenticatorRemoved(osmosis_authenticators::OnAuthenticatorRemovedRequest),
    Authenticate(osmosis_authenticators::AuthenticationRequest),
    Track(osmosis_authenticators::TrackRequest),
    ConfirmExecution(osmosis_authenticators::ConfirmExecutionRequest),
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
}

#[cw_serde]
pub struct SpendingResponse {
    pub spending: Spending,
}

#[cw_serde]
pub struct SpendingsByAccountResponse {
    pub spendings: Vec<(String, Spending)>,
}
