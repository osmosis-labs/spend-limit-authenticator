use cosmwasm_std::{DepsMut, Env, Response};
use osmosis_authenticators::OnAuthenticatorRemovedRequest;

use crate::{authenticator::AuthenticatorError, state::SPENDINGS};

use super::validate_and_parse_params;

pub fn on_authenticator_removed(
    deps: DepsMut,
    _env: Env,
    OnAuthenticatorRemovedRequest {
        account,
        authenticator_params,
    }: OnAuthenticatorRemovedRequest,
) -> Result<Response, AuthenticatorError> {
    let authenticator_params = validate_and_parse_params(authenticator_params)?;

    // clean up the spending
    SPENDINGS.remove(
        deps.storage,
        (&account, authenticator_params.authenticator_id.u64()),
    );

    Ok(Response::new())
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        testing::{mock_dependencies, mock_env},
        to_json_binary, Addr,
    };

    use crate::spend_limit::{Period, SpendLimitParams, Spending};

    use super::*;

    #[test]
    fn test_on_authenticator_removed() {
        let mut deps = mock_dependencies();

        // validate params
        let msg = OnAuthenticatorRemovedRequest {
            account: Addr::unchecked("account"),
            authenticator_params: None,
        };
        assert_eq!(
            on_authenticator_removed(deps.as_mut(), mock_env(), msg).unwrap_err(),
            AuthenticatorError::MissingAuthenticatorParams
        );

        // remove the spending
        let key = (&Addr::unchecked("account"), 2);
        SPENDINGS
            .save(deps.as_mut().storage, key, &Spending::default())
            .unwrap();
        assert_eq!(SPENDINGS.has(deps.as_ref().storage, key), true);

        let msg = OnAuthenticatorRemovedRequest {
            account: Addr::unchecked("account"),
            authenticator_params: Some(
                to_json_binary(&SpendLimitParams {
                    authenticator_id: 2u64.into(),
                    limit: "1000usdc".parse().unwrap(),
                    reset_period: Period::Day,
                })
                .unwrap(),
            ),
        };

        on_authenticator_removed(deps.as_mut(), mock_env(), msg).unwrap();
        assert_eq!(SPENDINGS.has(deps.as_ref().storage, key), false);
    }
}
