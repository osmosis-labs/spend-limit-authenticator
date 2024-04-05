use clap::{Parser, Subcommand, ValueEnum};

use colored::Colorize;
use cosmwasm_std::Decimal;
use indicatif::ProgressBar;
use inquire::{
    ui::{IndexPrefix, RenderConfig},
    Confirm, MultiSelect, Select,
};
use num_format::{Locale, ToFormattedString};
use sl_util::{
    arithmetic_twap_to_now, error::PrepError, get_pool_liquidities, get_pools, get_route,
    get_tokens, Config, PoolInfo, Result, TokenInfo,
};
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
    #[clap(visible_alias = "msg")]
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
    #[clap(visible_alias = "gen")]
    Generate {
        /// File to write resulted message to, if there is valid existing message,
        /// the default behavior is to continue from that state, except `--reset` flag is set.
        target_file: PathBuf,

        /// Mode for message generation, `continue` will continue from previous state,
        /// `reset` will start from scratch, `edit` will allow user to edit existing message.
        #[arg(long, default_value_t = Mode::Continue)]
        mode: Mode,

        /// By default, selecting a route requires going through manual twap price confirmation for sanity check.
        /// This flag will skip that confirmation.
        #[arg(long, default_value_t = false)]
        skip_manual_price_confirmation: bool,

        /// Filtering out route that contains pool that is blacklisted.
        /// There are some pools that are not cw pool yet failed to calculate twap.
        #[arg(long, value_delimiter = ',')]
        blacklisted_pools: Vec<u64>,

        /// Filtering out tracked denoms that its route contains newer pool
        /// than latest pool that gets synced from mainnet.
        /// This is only used for setting up test environment.
        #[arg(long)]
        latest_synced_pool: Option<u64>,

        /// Config file to use for message generation, if not provided, default config will be used.
        #[arg(long)]
        config: PathBuf,
    },
}

#[derive(clap::ValueEnum, Clone, Debug, PartialEq)]
pub enum Mode {
    Continue,
    Reset,
    Edit,
}

impl Display for Mode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Mode::Continue => write!(f, "continue"),
            Mode::Reset => write!(f, "reset"),
            Mode::Edit => write!(f, "edit"),
        }
    }
}

#[derive(Subcommand, Debug)]
enum TokenCommand {
    /// List tokens in the format that is easiliy copy-pastable to config.toml
    List {
        /// Sort tokens by
        #[arg(long, default_value_t = SortBy::Volume24h)]
        sort_by: SortBy,

        /// Include all infos for each token
        #[arg(long, short, default_value_t = false)]
        verbose: bool,
    },
}

#[derive(ValueEnum, Debug, Clone, Copy)]
enum SortBy {
    Volume24h,
    Liquidity,
}

impl Display for SortBy {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            SortBy::Volume24h => write!(f, "volume24h"),
            SortBy::Liquidity => write!(f, "liquidity"),
        }
    }
}

#[tokio::main]
async fn main() -> std::result::Result<(), String> {
    let args = Program::parse();

    match args.cmd {
        RootCommand::Message { cmd } => match cmd {
            MessageCommand::Generate {
                target_file,
                mode,
                skip_manual_price_confirmation,
                blacklisted_pools,
                latest_synced_pool,
                config,
            } => {
                let conf = toml::from_str(
                    &std::fs::read_to_string(config).map_err(|e| format!("😢 {}", e))?,
                )
                .map_err(|e| format!("😢 {}", e))?;

                select_routes(
                    conf,
                    target_file,
                    blacklisted_pools,
                    latest_synced_pool,
                    mode,
                    skip_manual_price_confirmation,
                )
                .await
                .map_err(|e| format!("😢 {}", e))?;
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

struct AssetChoice<'a>(RouteChoice<'a>);

impl Display for AssetChoice<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let token_in_symbol = self.0.token_map[self.0.token_in].symbol.as_str();
        write!(f, "{} | {}", token_in_symbol, self.0)
    }
}

async fn select_routes(
    conf: Config,
    target_file: PathBuf,
    blacklisted_pools: Vec<u64>,
    latest_synced_pool: Option<u64>,
    mode: Mode,
    skip_manual_price_confirmation: bool,
) -> Result<()> {
    let config_only_msg = InstantiateMsg {
        price_resolution_config: conf.price_resolution.clone(),
        tracked_denoms: vec![],
    };

    let mut msg: InstantiateMsg = if target_file.exists() {
        match mode {
            Mode::Reset => config_only_msg,
            Mode::Continue | Mode::Edit => {
                let msg =
                    std::fs::read_to_string(target_file.clone()).expect("Failed to read file");
                let msg: InstantiateMsg = serde_json::from_str(&msg).expect("Failed to parse msg");
                // test if denoms in state has denoms that are not in config
                for tracked_denom in msg.tracked_denoms.iter() {
                    let denom_does_not_appear_in_config =
                        !conf.tracked_denoms.contains(&tracked_denom.denom);
                    if denom_does_not_appear_in_config {
                        return Err(PrepError::InvalidState {
                            denom: tracked_denom.denom.clone(),
                        }
                        .into());
                    }
                }
                msg
            }
        }
    } else {
        config_only_msg
    };

    let mut total_selection_count = conf.tracked_denoms.len();
    let mut progress = msg.tracked_denoms.len();
    let existing_tracked_denoms = msg.tracked_denoms.clone();

    let tracked_denoms = msg.tracked_denoms.clone();
    let denom_refs: HashMap<_, _> = tracked_denoms
        .iter()
        .map(|tracked| (tracked.denom.as_str(), tracked.denom.as_str()))
        .collect();

    // fetch token info, pool info, and pool liquidity
    let spinner = ProgressBar::new_spinner();

    spinner.set_message("Fetching token info...");
    let token_map = get_token_map().await.expect("Failed to get prices");

    spinner.set_message("Fetching general pool info...");
    let pool_infos = get_pools().await.expect("Failed to get pools");

    spinner.set_message("Fetching pools' liquidity...");
    let liquidities = get_pool_liquidities()
        .await
        .expect("Failed to get pool liquidities");

    spinner.finish_and_clear();

    let denoms: Box<dyn Iterator<Item = String>> = match mode {
        Mode::Continue => {
            // find vec of denoms that are not yet selected
            let pending_denoms = conf.tracked_denoms.into_iter().filter(|denom| {
                !existing_tracked_denoms
                    .iter()
                    .any(|tracked| tracked.denom == *denom)
            });
            Box::new(pending_denoms)
        }
        Mode::Reset => Box::new(conf.tracked_denoms.into_iter()),
        Mode::Edit => {
            let route_choices = msg
                .tracked_denoms
                .clone()
                .into_iter()
                .map(|tracked_denom| {
                    AssetChoice(RouteChoice {
                        token_in: denom_refs[tracked_denom.denom.as_str()],
                        routes: tracked_denom.swap_routes,
                        token_map: &token_map,
                        pool_infos: &pool_infos,
                        liquidities: &liquidities,
                    })
                })
                .collect::<Vec<_>>();

            let editing_denoms = MultiSelect::new(
                "Select tracked denoms & their routes to edit",
                route_choices,
            )
            .with_render_config(
                RenderConfig::default().with_option_index_prefix(IndexPrefix::SpacePadded),
            )
            .prompt()
            .unwrap()
            .into_iter()
            .map(|asset_choice| asset_choice.0.token_in.to_string())
            .collect::<Vec<_>>();

            // filter edit denoms out of  existing tracked denoms
            msg.tracked_denoms = existing_tracked_denoms
                .into_iter()
                .filter(|tracked| !editing_denoms.contains(&tracked.denom))
                .collect();

            // for edit mode, progress starts from 0 since it only cares about what will be edited, not the whole denoms in config
            progress = 0;
            // for edit mode, selection count is the count of denoms that are to be edited
            total_selection_count = editing_denoms.len();

            Box::new(editing_denoms.into_iter())
        }
    };

    for (index, denom) in denoms.enumerate() {
        let qoute_denom = conf.price_resolution.quote_denom.to_string();
        let pool_infos = pool_infos.clone();
        let blacklisted_pools = blacklisted_pools.clone();

        let spinner = ProgressBar::new_spinner();
        spinner.set_message("Fetching available routes...");
        let swap_routes = get_route(
            denom.to_string().as_str(),
            qoute_denom.as_str(),
            blacklisted_pools,
            latest_synced_pool,
            &pool_infos,
        )
        .await
        .expect("Failed to get route");

        spinner.finish_and_clear();

        // select route
        'select_route: loop {
            let route_choices = swap_routes
                .clone()
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
                    progress + index + 1,
                    total_selection_count,
                    symbol
                )
                .as_str(),
                route_choices,
            )
            .with_render_config(
                RenderConfig::default().with_option_index_prefix(IndexPrefix::SpacePadded),
            )
            .prompt()?;

            let mut token_in_denom = denom.to_string();
            let mut resulted_price = Decimal::one();
            for route in route_choice.routes.iter() {
                let spinner = ProgressBar::new_spinner();
                spinner.set_message(format!(
                    "Try fetching twap for {}/{} on pool {} ...",
                    token_in_denom, route.token_out_denom, route.pool_id
                ));
                let twap_res = arithmetic_twap_to_now(
                    route.pool_id,
                    token_in_denom.as_str(),
                    route.token_out_denom.as_str(),
                    time::OffsetDateTime::now_utc()
                        .checked_sub(time::Duration::hours(1))
                        .unwrap(),
                )
                .await;

                spinner.finish_and_clear();

                let token_in_symbol = token_map[&token_in_denom].symbol.as_str();
                let token_out_symbol = token_map[&route.token_out_denom].symbol.as_str();
                match twap_res {
                    Ok(twap) => {
                        // if routes has only 1 hop, just confirm the resulted price
                        // or else it will just appears to be duplicated confirmation
                        if route_choice.routes.len() > 1 {
                            if skip_manual_price_confirmation {
                                println!(
                                    "\t#{} {}/{} = {}",
                                    route.pool_id, token_in_symbol, token_out_symbol, twap
                                );
                            } else {
                                let confirm = Confirm::new(&format!(
                                    "#{} {}/{} = {}, OK?",
                                    route.pool_id, token_in_symbol, token_out_symbol, twap
                                ))
                                .prompt()?;

                                // if not ok, restart selecting route
                                if !confirm {
                                    continue 'select_route;
                                }
                            }
                        }

                        // update resulted price for next route
                        resulted_price = resulted_price * twap
                    }
                    Err(e) => {
                        let m = format!(
                            "⚠️ Failed to fetch twap for {}/{}: {}. Try another route?",
                            token_in_symbol, token_out_symbol, e
                        );
                        eprintln!("{}", m.yellow().bold());
                        // if failed to fetch twap, restart selecting route
                        continue 'select_route;
                    }
                }

                // update token_in_denom for next route
                token_in_denom = route.token_out_denom.clone();
            }

            if skip_manual_price_confirmation {
                println!(
                    "\t🪙 {}/{} = {}",
                    token_map[denom.as_str()].symbol,
                    token_map[qoute_denom.as_str()].symbol,
                    resulted_price
                );
            } else {
                let confirm = Confirm::new(&format!(
                    "🪙 {}/{} = {}, OK?",
                    token_map[denom.as_str()].symbol,
                    token_map[qoute_denom.as_str()].symbol,
                    resulted_price
                ))
                .prompt()?;

                // if not ok, restart selecting route
                if !confirm {
                    continue 'select_route;
                }
            }

            let res = TrackedDenom {
                denom: denom.to_string(),
                swap_routes: route_choice.routes,
            };

            msg.tracked_denoms.push(res);
            break;
        }

        // keep saving result to file every time user selects a route
        let msg = serde_json::to_string_pretty(&msg).expect("Failed to serialize msg");
        std::fs::write(target_file.clone(), msg).expect("Failed to write msg to file");
    }

    println!(
        "{}",
        match mode {
            Mode::Reset | Mode::Continue => "📟 Message generation completed!",
            Mode::Edit => "✍️ Message editing completed!",
        }
    );
    return Ok(());
}

async fn get_token_map() -> Result<BTreeMap<String, TokenInfo>> {
    let tokens = get_tokens().await?;
    let prices = tokens
        .into_iter()
        .map(|token| (token.denom.clone(), token))
        .collect::<BTreeMap<_, _>>();

    Ok(prices)
}
