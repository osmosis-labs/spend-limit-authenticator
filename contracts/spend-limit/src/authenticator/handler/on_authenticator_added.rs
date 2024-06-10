use cosmwasm_std::{ensure, DepsMut, Env, Response};
use cw_authenticator::OnAuthenticatorAddedRequest;

use crate::{
    authenticator::{handler::validate_and_parse_params, AuthenticatorError},
    spend_limit::Spending,
    state::SPENDINGS,
};

pub fn on_authenticator_added(
    deps: DepsMut,
    _env: Env,
    OnAuthenticatorAddedRequest {
        authenticator_id,
        account,
        authenticator_params,
    }: OnAuthenticatorAddedRequest,
) -> Result<Response, AuthenticatorError> {
    let _ = validate_and_parse_params(authenticator_params)?;

    // Make sure (account, authenticator_id) is not already present in the state
    let key = (&account, authenticator_id.as_str());
    ensure!(
        !SPENDINGS.has(deps.storage, key),
        AuthenticatorError::authenticator_already_exists(account, authenticator_id.as_str())
    );

    // initialize the spending for this authenticator
    SPENDINGS.save(deps.storage, key, &Spending::default())?;

    Ok(Response::new().add_attribute("action", "on_authenticator_added"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::period::Period;
    use crate::spend_limit::SpendLimitParams;
    use cosmwasm_std::{
        testing::{mock_dependencies_with_balances, mock_env},
        to_json_binary, Addr, Coin, StdError, Uint128,
    };

    const USDC: &str = "ibc/498A0751C798A0D9A389AA3691123DADA57DAA4FE165D5C75894505B876BA6E4";

    #[test]
    fn test_on_authenticator_added() {
        let mut deps = mock_dependencies_with_balances(&[("someoneelse", &[Coin::new(1, USDC)])]);

        // missing authenticator_params
        let request = OnAuthenticatorAddedRequest {
            authenticator_id: "2".to_string(),
            account: Addr::unchecked("addr"),
            authenticator_params: None,
        };
        assert_eq!(
            on_authenticator_added(deps.as_mut(), mock_env(), request).unwrap_err(),
            AuthenticatorError::MissingAuthenticatorParams
        );

        // invalid authenticator_params
        let request = OnAuthenticatorAddedRequest {
            authenticator_id: "2".to_string(),
            account: Addr::unchecked("addr"),
            authenticator_params: Some(to_json_binary(&"invalid").unwrap()),
        };

        assert_eq!(
            on_authenticator_added(deps.as_mut(), mock_env(), request).unwrap_err(),
            AuthenticatorError::invalid_authenticator_params(StdError::parse_err(
                std::any::type_name::<SpendLimitParams>(),
                "Invalid type"
            ))
        );

        // valid
        let request = OnAuthenticatorAddedRequest {
            authenticator_id: "2".to_string(),
            account: Addr::unchecked("addr"),
            authenticator_params: Some(
                to_json_binary(&SpendLimitParams {
                    limit: Uint128::new(500_000_000),
                    reset_period: Period::Day,
                    time_limit: None,
                })
                .unwrap(),
            ),
        };

        let res = on_authenticator_added(deps.as_mut(), mock_env(), request).unwrap();
        assert_eq!(
            res,
            Response::new().add_attribute("action", "on_authenticator_added")
        );

        // check the state
        let spending = SPENDINGS
            .load(deps.as_ref().storage, (&Addr::unchecked("addr"), "2"))
            .unwrap();
        assert_eq!(spending, Spending::default());

        // Adding the authenticator with the same (account, authenticator_id) should fail
        let request = OnAuthenticatorAddedRequest {
            authenticator_id: "2".to_string(),
            account: Addr::unchecked("addr"),
            authenticator_params: Some(
                to_json_binary(&SpendLimitParams {
                    limit: Uint128::new(500_000_000),
                    reset_period: Period::Month,
                    time_limit: None,
                })
                .unwrap(),
            ),
        };

        assert_eq!(
            on_authenticator_added(deps.as_mut(), mock_env(), request).unwrap_err(),
            AuthenticatorError::authenticator_already_exists(Addr::unchecked("addr"), "2")
        );
    }
}
