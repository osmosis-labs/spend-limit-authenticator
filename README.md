# Spend Limit Authenticator

Spend limit Authenticator is a CosmWasm [authenticator](https://github.com/osmosis-labs/osmosis/tree/feat/smart-accounts/x/authenticator) that allows users/application to set a spend limit on an account they own. The spend limit is enforced by the authenticator, and can be changed by the user at any time.

- price strategy
- period type

once period is over, reset the quota
