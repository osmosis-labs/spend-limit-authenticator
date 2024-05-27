mod error;
mod params;
use cosmwasm_std::DepsMut;

use crate::error::ContractError;
pub use error::PasskeyError;
pub use params::PasskeyParams;

#[allow(clippy::too_many_arguments)]
pub fn update_and_check_passkey(
    mut _deps: DepsMut,
    _params: &PasskeyParams,
) -> Result<(), ContractError> {
    Ok(())
}
