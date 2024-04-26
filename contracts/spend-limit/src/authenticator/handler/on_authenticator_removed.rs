use cosmwasm_std::{DepsMut, Env, Response};
use osmosis_authenticators::OnAuthenticatorRemovedRequest;

use crate::{authenticator::AuthenticatorError, state::SPENDINGS};

pub fn on_authenticator_removed(
    deps: DepsMut,
    _env: Env,
    OnAuthenticatorRemovedRequest {
        account,
        authenticator_id,
        ..
    }: OnAuthenticatorRemovedRequest,
) -> Result<Response, AuthenticatorError> {
    // clean up the spending
    SPENDINGS.remove(deps.storage, (&account, authenticator_id.as_str()));

    Ok(Response::new().add_attribute("action", "on_authenticator_removed"))
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        testing::{mock_dependencies, mock_env},
        to_json_binary, Addr,
    };

    use crate::period::Period;
    use crate::spend_limit::{SpendLimitParams, Spending};

    use super::*;

    #[test]
    fn test_on_authenticator_removed() {
        let mut deps = mock_dependencies();

        // remove the spending
        let key = (&Addr::unchecked("account"), "2");
        SPENDINGS
            .save(deps.as_mut().storage, key, &Spending::default())
            .unwrap();
        assert!(SPENDINGS.has(deps.as_ref().storage, key));

        let msg = OnAuthenticatorRemovedRequest {
            authenticator_id: "2".to_string(),
            account: Addr::unchecked("account"),
            authenticator_params: Some(
                to_json_binary(&SpendLimitParams {
                    limit: 1000u128.into(),
                    reset_period: Period::Day,
                    time_limit: None,
                })
                .unwrap(),
            ),
        };

        on_authenticator_removed(deps.as_mut(), mock_env(), msg).unwrap();
        assert!(!SPENDINGS.has(deps.as_ref().storage, key));
    }
}
