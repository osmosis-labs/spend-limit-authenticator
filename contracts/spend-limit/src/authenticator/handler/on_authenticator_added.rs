use cosmwasm_std::{ensure, DepsMut, Env, Response};
use osmosis_authenticators::OnAuthenticatorAddedRequest;

use crate::{
    authenticator::{handler::validate_and_parse_params, AuthenticatorError},
    spend_limit::Spending,
    state::SPENDINGS,
};

pub fn on_authenticator_added(
    deps: DepsMut,
    _env: Env,
    OnAuthenticatorAddedRequest {
        account,
        authenticator_params,
    }: OnAuthenticatorAddedRequest,
) -> Result<Response, AuthenticatorError> {
    let authenticator_params = validate_and_parse_params(authenticator_params)?;

    // Make sure if denom has any supply at all
    let supply = deps
        .querier
        .query_supply(authenticator_params.limit.denom.as_str())?;

    ensure!(
        !supply.amount.is_zero(),
        AuthenticatorError::invalid_denom(authenticator_params.limit.denom.as_str())
    );

    // Make sure (account, authenticator_params.subkey) is not already present in the state
    let key = (&account, authenticator_params.subkey.as_str());
    ensure!(
        !SPENDINGS.has(deps.storage, key),
        AuthenticatorError::authenticator_already_exists(
            account,
            authenticator_params.subkey.as_str()
        )
    );

    // initialize the spending for this authenticator
    SPENDINGS.save(deps.storage, key, &Spending::default())?;

    Ok(Response::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spend_limit::{Period, SpendLimitParams};
    use cosmwasm_std::{
        testing::{mock_dependencies_with_balances, mock_env},
        to_json_binary, Addr, Coin, StdError,
    };

    const USDC: &str = "ibc/498A0751C798A0D9A389AA3691123DADA57DAA4FE165D5C75894505B876BA6E4";

    #[test]
    fn test_on_authenticator_added() {
        let mut deps = mock_dependencies_with_balances(&[("someoneelse", &[Coin::new(1, USDC)])]);

        // missing authenticator_params
        let request = OnAuthenticatorAddedRequest {
            account: Addr::unchecked("addr"),
            authenticator_params: None,
        };
        assert_eq!(
            on_authenticator_added(deps.as_mut(), mock_env(), request).unwrap_err(),
            AuthenticatorError::MissingAuthenticatorParams
        );

        // invalid authenticator_params
        let request = OnAuthenticatorAddedRequest {
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

        // invalid denom
        let request = OnAuthenticatorAddedRequest {
            account: Addr::unchecked("addr"),
            authenticator_params: Some(
                to_json_binary(&SpendLimitParams {
                    subkey: "500invalid_denom".to_string(),
                    limit: Coin::new(500__000_000, "invalid_denom"),
                    reset_period: Period::Day,
                })
                .unwrap(),
            ),
        };

        assert_eq!(
            on_authenticator_added(deps.as_mut(), mock_env(), request).unwrap_err(),
            AuthenticatorError::invalid_denom("invalid_denom")
        );

        // valid
        let request = OnAuthenticatorAddedRequest {
            account: Addr::unchecked("addr"),
            authenticator_params: Some(
                to_json_binary(&SpendLimitParams {
                    subkey: "500usdc".to_string(),
                    limit: Coin::new(500__000_000, USDC),
                    reset_period: Period::Day,
                })
                .unwrap(),
            ),
        };

        let res = on_authenticator_added(deps.as_mut(), mock_env(), request).unwrap();
        assert_eq!(res, Response::new());

        // check the state
        let spending = SPENDINGS
            .load(deps.as_ref().storage, (&Addr::unchecked("addr"), "500usdc"))
            .unwrap();
        assert_eq!(spending, Spending::default());

        // Adding the authenticator with the same (account, subkey) should fail
        let request = OnAuthenticatorAddedRequest {
            account: Addr::unchecked("addr"),
            authenticator_params: Some(
                to_json_binary(&SpendLimitParams {
                    subkey: "500usdc".to_string(),
                    limit: Coin::new(500__000_000, USDC),
                    reset_period: Period::Month,
                })
                .unwrap(),
            ),
        };

        assert_eq!(
            on_authenticator_added(deps.as_mut(), mock_env(), request).unwrap_err(),
            AuthenticatorError::authenticator_already_exists(Addr::unchecked("addr"), "500usdc")
        );
    }
}
