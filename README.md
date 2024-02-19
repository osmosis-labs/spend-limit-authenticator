# Spend Limit Authenticator

Spend limit Authenticator is a CosmWasm [authenticator](https://github.com/osmosis-labs/osmosis/tree/feat/smart-accounts/x/authenticator) that allows users/application to set a spend limit on their account authentication path (each account can have multiple root authenticator, account-wide limit is currently not supported).

## Overview

Each spend limit authenticator must have [SpendLimitParams](./contracts/spend-limit/src/spend_limit/params.rs) as authenticator params. This is act as a parameter for each specific instance of the authenticator and stored in the module state. It will be passed along to to its hooks and can be used to enforce the spend limit.

Other global configurations and states are stored in the [contract state](./contracts/spend-limit/src/state.rs).

For each transaction, the authenticator will check if the transaction amount is within the spend limit. If the transaction amount is greater than the spend limit, the transaction will be rejected. Here is the breakdown:

- For this authenticator, it is always `Authenticated` in `Authenticate` hook, since it will check the spend limit in `ConfirmExecution` hook.
- It checks by using `Track` hook to get the pre execution balances of the account.
- then, `ConfirmExecution` hook get the post execution balances of the account. The difference between the pre and post execution balances is the amount spent in the transaction.
- If last spending update was within the past set period, it resets the spending to 0.
- The amount spent are then converted into quoted denom using TWAP price.
- If the amount spent is greater than the spend limit, the transaction will be rejected. If not, it will be accepted and the spending will be accumulated.

## Development

### Pre-requisites

- [Rust](https://www.rust-lang.org/)
- [Go](https://golang.org/) (for running integration tests & localosmosis)
- [CosmWasm Setup](https://book.cosmwasm.com/setting-up-env.html)
- [Beaker](https://github.com/osmosis-labs/beaker)
- [Docker](https://www.docker.com/)

### Build

Building wasm binary for testing:

```sh
beaker wasm build --no-wasm-opt
```

Note that the flag `--no-wasm-opt` is used to disable wasm-opt optimization. This is useful for debugging and testing and small enough since debug symbols are stripped, it's not recommended for production. Omit this flag for production build.

Output wasm bytecode is stored at `target/wasm32-unknown-unknown/release/spend_limit_authenticator.wasm`.

### Testing

This repo has automated unit testing as well as integration tests using [`test-tube`](https://github.com/osmosis-labs/test-tube). `test-tube` requires the above artifacts to be built in order to run the tests.
