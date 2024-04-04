use clap::{Parser, Subcommand, ValueEnum};
use inquire::{
    ui::{IndexPrefix, RenderConfig},
    Select,
};
use num_format::{Locale, ToFormattedString};
use prep_instantiate::{
    get_pool_liquidities, get_pools, get_route, get_tokens, Config, PoolInfo, Result, TokenInfo,
};
use serde::Serialize;
use spend_limit::msg::{InstantiateMsg, SwapAmountInRoute, TrackedDenom};
use std::{
    collections::{BTreeMap, HashMap},
    fmt::{self, Display, Formatter},
    path::PathBuf,
};

#[derive(Parser)]
#[clap(author, version,about, long_about = None)]
#[clap(propagate_version = true)]
pub struct Program {
    #[clap(subcommand)]
    pub(crate) cmd: RootCommand,
}

#[derive(Subcommand, Debug)]
enum RootCommand {
    /// Message generation/editing commands
    #[clap(alias = "msg")]
    Message {
        #[clap(subcommand)]
        cmd: MessageCommand,
    },

    /// Token related commands
    Token {
        #[clap(subcommand)]
        cmd: TokenCommand,
    },
}

#[derive(Subcommand, Debug)]
enum MessageCommand {
    /// Generate instantiate msg for spend-limit contract
    #[clap(alias = "gen")]
    Generate {
        /// File to write resulted message to, if there is valid existing message,
        /// the default behavior is to continue from that state, except `--reset` flag is set.
        target_file: PathBuf,

        /// Ensure that message generation always starts from scratch instead of continuing from
        /// previous state.
        #[arg(long, default_value_t = false)]
        reset: bool,

        /// Filtering out route that contains pool that is blacklisted.
        /// There are some pools that are not cw pool yet failed to calculate twap.
        #[arg(long, value_delimiter = ',')]
        blacklisted_pools: Vec<u64>,

        /// Filtering out tracked denoms that its route contains newer pool
        /// than latest pool that gets synced from mainnet.
        /// This is only used for setting up test environment.
        #[arg(long)]
        latest_synced_pool: Option<u64>,
    },
}

#[derive(Subcommand, Debug)]
enum TokenCommand {
    /// List tokens in the format that is easiliy copy-pastable to config.toml
    List {
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
    let args = Program::parse();

    match args.cmd {
        RootCommand::Message { cmd } => match cmd {
            MessageCommand::Generate {
                target_file,
                reset,
                blacklisted_pools,
                latest_synced_pool,
            } => {
                // TODO: expose config file location as an argument
                let conf: Config = toml::from_str(include_str!("../config.toml"))?;

                select_routes(
                    conf,
                    target_file,
                    blacklisted_pools,
                    latest_synced_pool,
                    reset,
                )
                .await;
            }
        },
        RootCommand::Token { cmd } => match cmd {
            TokenCommand::List { sort_by, verbose } => {
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
        },
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

struct RouteChoice<'a> {
    token_in: &'a str,
    routes: Vec<SwapAmountInRoute>,
    token_map: &'a BTreeMap<String, TokenInfo>,
    pool_infos: &'a HashMap<u64, PoolInfo>,
    liquidities: &'a HashMap<u64, f64>,
}

impl<'a> Display for RouteChoice<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let token_in_symbol = self.token_map[self.token_in].symbol.as_str();

        let hops = self.routes.len();

        write!(f, "[hops = {}] -({})>", hops, token_in_symbol)?;

        for route in self.routes.iter() {
            let token_out_symbol = self.token_map[&route.token_out_denom].symbol.as_str();
            let pool_info = self.pool_infos.get(&route.pool_id).unwrap();
            let liquidity = (self.liquidities.get(&route.pool_id).unwrap().round() as u64)
                .to_formatted_string(&Locale::en);

            write!(
                f,
                " pool:[{}#{} ~ ${}] -({})>",
                pool_info.pool_type, route.pool_id, liquidity, token_out_symbol
            )?;
        }

        Ok(())
    }
}

async fn select_routes(
    conf: Config,
    target_file: PathBuf,
    blacklisted_pools: Vec<u64>,
    latest_synced_pool: Option<u64>,
    reset: bool,
) {
    let prog = indicatif::ProgressBar::new_spinner();

    prog.set_message("Fetching token info...");
    let token_map = get_token_map().await.expect("Failed to get prices");

    prog.set_message("Fetching general pool info...");
    let pool_infos = get_pools().await.expect("Failed to get pools");

    prog.set_message("Fetching pools' liquidity...");
    let liquidities = get_pool_liquidities()
        .await
        .expect("Failed to get pool liquidities");

    prog.finish_and_clear();

    let total_denoms = conf.tracked_denoms.len();

    // if not reset, it will try to continue from previous state if exists
    let mut msg: InstantiateMsg = if !reset && target_file.exists() {
        let msg = std::fs::read_to_string(target_file.clone()).expect("Failed to read file");
        serde_json::from_str(&msg).expect("Failed to parse msg")
    } else {
        InstantiateMsg {
            price_resolution_config: conf.price_resolution.clone(),
            tracked_denoms: vec![],
        }
    };

    let prev_progress = msg.tracked_denoms.len();
    let existing_tracked_denoms = msg.tracked_denoms.clone();

    let denoms: Box<dyn Iterator<Item = String>> = if reset {
        Box::new(conf.tracked_denoms.into_iter())
    } else {
        // find vec of denoms that are not yet selected
        let pending_denoms = conf.tracked_denoms.into_iter().filter(|denom| {
            !existing_tracked_denoms
                .iter()
                .any(|tracked| tracked.denom == *denom)
        });
        Box::new(pending_denoms)
    };

    for (index, denom) in denoms.enumerate() {
        let qoute_denom = conf.price_resolution.quote_denom.to_string();
        let pool_infos = pool_infos.clone();
        let blacklisted_pools = blacklisted_pools.clone();

        let prog = indicatif::ProgressBar::new_spinner();
        prog.set_message("Fetching available routes...");
        let swap_routes = get_route(
            denom.to_string().as_str(),
            qoute_denom.as_str(),
            blacklisted_pools,
            latest_synced_pool,
            &pool_infos,
        )
        .await
        .expect("Failed to get route");

        prog.finish_and_clear();

        let route_choices = swap_routes
            .into_iter()
            .map(|routes| RouteChoice {
                token_in: denom.as_str(),
                routes,
                token_map: &token_map,
                pool_infos: &pool_infos,
                liquidities: &liquidities,
            })
            .collect::<Vec<_>>();

        let symbol = token_map[denom.as_str()].symbol.as_str();
        let route_choice = Select::new(
            format!(
                "<{}/{}> `{}` route =",
                prev_progress + index + 1,
                total_denoms,
                symbol
            )
            .as_str(),
            route_choices,
        )
        .with_render_config(
            RenderConfig::default().with_option_index_prefix(IndexPrefix::SpacePadded),
        )
        .prompt()
        .unwrap();

        let res = TrackedDenom {
            denom: denom.to_string(),
            swap_routes: route_choice.routes,
        };

        msg.tracked_denoms.push(res);

        // keep saving result to file every time user selects a route
        let msg = serde_json::to_string_pretty(&msg).expect("Failed to serialize msg");
        std::fs::write(target_file.clone(), msg).expect("Failed to write msg to file");
    }

    println!("📟 Message generation completed!");
}

async fn get_token_map() -> Result<BTreeMap<String, TokenInfo>> {
    let tokens = get_tokens().await?;
    let prices = tokens
        .into_iter()
        .map(|token| (token.denom.clone(), token))
        .collect::<BTreeMap<_, _>>();

    Ok(prices)
}
