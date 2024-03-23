# Prepararing Instantiate Message

This is program that prepares the instantiate message for the spend limit contract.

It finds best routes for each tracked denom through [sqs](https://github.com/osmosis-labs/sqs) to construct the instantiate message alongside with other necessary information defined in [config.toml](./config.toml).

To run the program, edit [config.toml](./config.toml) to fits your needs and simply `cargo run`, resulting in the instantiate message which will be written to stdout.

Feel free to redirect it to a file

```bash
cargo run > instantiate-msg.json
```

or use it directly in the `osmosisd tx wasm instantiate` command.

```bash
osmosisd tx wasm instantiate $CODE_ID $(cargo run) --label "spend-limit" --no-admin --gas-prices 0.25uosmo --gas auto --gas-adjustment 1.5 --from $ACCOUNT
```
