use clap::{Parser, Subcommand, ValueEnum};
use futures::StreamExt;
use prep_instantiate::{get_route, get_tokens, Config, Result, Token, TokenInfo};
use serde::Serialize;
use spend_limit::msg::{InstantiateMsg, TrackedDenom};
use tokio::task::JoinHandle;

/// Prepare instantiate msg for spend-limit contract
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Generate instantiate msg for spend-limit contract
    GenMsg {
        /// Number of concurrent requests to make to get route
        #[arg(long, default_value_t = 10)]
        concurrency: usize,
    },

    /// List tokens in the format that is easiliy copy-pastable to config.toml
    ListTokens {
        /// Sort tokens by
        #[arg(long)]
        sort_by: SortBy,

        /// Include all infos for each token
        #[arg(long, short, default_value_t = false)]
        verbose: bool,
    },
}

#[derive(ValueEnum, Debug, Default, Serialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
enum SortBy {
    #[default]
    Volume24h,
    Liquidity,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::GenMsg { concurrency } => {
            let conf: Config = toml::from_str(include_str!("../config.toml"))?;

            let tracked_denoms = get_tracked_denom_infos(
                conf.tracked_denoms.clone(),
                conf.routing_amount_in
                    .parse()
                    .expect("Failed to parse routing amount in as u128"),
                &conf.price_resolution.quote_denom,
                concurrency,
            )
            .await;

            let msg = InstantiateMsg {
                price_resolution_config: conf.price_resolution,
                tracked_denoms,
            };

            // write instantiate msg to stdout
            let msg_str = serde_json::to_string(&msg)?;
            println!("{}", msg_str);
        }
        Commands::ListTokens { sort_by, verbose } => {
            get_tokens_sorted_by_24h_volume(sort_by)
                .await
                .iter()
                .for_each(|token| {
                    print!("\"{}\", # {} - {}", token.denom, token.symbol, token.name);
                    if verbose {
                        print!(
                            " (volume_24h = {}, liquidity = {})",
                            token.volume_24h, token.liquidity
                        );
                    }
                    println!();
                });
        }
    }

    Ok(())
}

async fn get_tokens_sorted_by_24h_volume(sort_by: SortBy) -> Vec<TokenInfo> {
    let mut tokens = get_tokens().await.expect("Failed to get tokens");

    match sort_by {
        SortBy::Volume24h => tokens.sort_by(|a, b| b.volume_24h.total_cmp(&a.volume_24h)),
        SortBy::Liquidity => tokens.sort_by(|a, b| b.liquidity.total_cmp(&a.liquidity)),
    }

    tokens
}

async fn get_tracked_denom_infos(
    denoms: Vec<String>,
    routing_amount_in: u128,
    qoute_denom: &str,
    concurrency: usize,
) -> Vec<TrackedDenom> {
    futures::stream::iter(denoms.into_iter().map(|denom| {
        let qoute_denom = qoute_denom.to_string();
        let handle: JoinHandle<TrackedDenom> = tokio::spawn(async move {
            let swap_routes = get_route(
                Token::new(routing_amount_in, denom.as_str()),
                qoute_denom.as_str(),
            )
            .await
            .expect("Failed to get route");

            TrackedDenom {
                denom: denom.to_string(),
                swap_routes,
            }
        });

        handle
    }))
    .buffer_unordered(concurrency)
    .map(|handle| handle.expect("Failed to join handle"))
    .collect::<Vec<_>>()
    .await
}
