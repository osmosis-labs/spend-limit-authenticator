# Prepararing Instantiate Message

This is program that prepares the instantiate message for the spend limit contract.

It finds best routes for each tracked denom through [sqs](https://github.com/osmosis-labs/sqs) to construct the instantiate message alongside with other necessary information defined in [config.toml](./config.toml).

To run the program, edit [config.toml](./config.toml) to fits your needs.

As a tool to help coming up with comprehensive tracked denoms in [config.toml](./config.toml), you can use `cargo run list-tokens` (use `-h` to see more options).
This will list all tokens avaialble through [imparator's api](https://api-osmosis.imperator.co/swagger/) in the format that is copy-pasteable to [config.toml](./config.toml).

```bash
cargo run list-tokens
```

`cargo run gen-msg` this generates instantiate message which will be written to stdout, so it can be redirected to a file

```bash
cargo run gen-msg > instantiate-msg.json
```

or use it directly in the `osmosisd tx wasm instantiate` command.

```bash
osmosisd tx wasm instantiate $CODE_ID $(cargo run gen-msg) --label "spend-limit" --no-admin --gas-prices 0.25uosmo --gas auto --gas-adjustment 1.5 --from $ACCOUNT
```
