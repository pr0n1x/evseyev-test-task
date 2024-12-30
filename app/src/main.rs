use std::fmt::Display;
use core::{fmt::Debug, error::Error};

use snafu::{Snafu, ResultExt as _};
use clap::{Parser as _, CommandFactory as _};

mod cli;
mod config;
mod cmd;
mod wallet;
mod token;
mod worker;

use cli::{Cli, SubCmd};
use crate::cli::{TestSubCmd, TestTransferSubCmd, TokenSubCmd, WalletSubCmd};

#[tokio::main]
async fn main() -> Result<(), FormattedMainError> {
    match try_main().await {
        Ok(x) => Ok(x),
        Err(err) => {
            match &err {
                MainError::CliParseError { source }=> {
                    match source.kind() {
                        clap::error::ErrorKind::DisplayHelp => {
                            let _ = source.print();
                            return Ok(())
                        }
                        _ => {},
                    }
                }
                _ => {},
            }
            Err(FormattedMainError{main_error: err})
        }
    }
}

async fn try_main() -> MainResult<()> {
    let cli = Cli::try_parse().context(CliParseSnafu)?;

    // some commands should be handled before config parsing
    match cli.command {
        // It's not necessary for the test task, but I use this block quite often, so let it be.
        SubCmd::Autocompletion {shell, ref cmd_name } => {
            let generator = shell.unwrap_or(clap_complete::Shell::Bash);
            let mut cmd = Cli::command();
            let cmd_name = cmd_name
                .clone()
                .unwrap_or_else(|| cmd.get_name().to_string());
            eprintln!("Generating completion file for {:?}...", generator);
            clap_complete::generate(generator, &mut cmd, cmd_name, &mut std::io::stdout());
            return Ok(());
        }
        SubCmd::Wallet { ref command } => match command.clone() {
            WalletSubCmd::Generate { count, save_to } => {
                return cmd::generate_wallets(count, save_to).await
            }
            WalletSubCmd::Read { path } => {
                println!("{}", wallet::convert_keypair_file_to_base58_string(path.as_path()).await.context(WalletSnafu)?);
                return Ok(())
            }
            _ => {}
        },
        _ => {}
    };


    let cmd = cmd::CmdHandlers::new(config::Config::try_from_cli(&cli).await.context(ConfigSnafu)?);

    match cli.command {
        SubCmd::Autocompletion { .. } => unreachable!("autocompletion subcommands should be already handled"),
        SubCmd::Wallet { command } => match command {
            WalletSubCmd::Generate { .. }
            | WalletSubCmd::Read { .. }  => unreachable!("some wallet subcommands should be already handled"),
            WalletSubCmd::List { pubkey, keypair } => cmd.print_wallets(pubkey, keypair),
            WalletSubCmd::Save { target } => cmd.save_wallets_to(target.as_path()).await,
        },
        SubCmd::ShowConfig => cmd.show_config(),
        SubCmd::Balances => cmd.print_sol_balances().await,
        SubCmd::Airdrop { sols, confirm } => cmd.airdrop(sols, confirm).await,
        SubCmd::Token { command } => match command {
            TokenSubCmd::Deploy => cmd.deploy_token().await,
            TokenSubCmd::Mint { holder, amount } => cmd.mint_to(holder, amount).await,
            TokenSubCmd::Balances => cmd.token_balances().await,
        },
        SubCmd::Test { command} => match command {
            TestSubCmd::Transfer { command } => match command {
                TestTransferSubCmd::Sols => cmd.test_batched_sols_transfer().await,
                TestTransferSubCmd::Tokens => cmd.test_batched_tokens_transfer().await,
            }
        }
    }
}

pub(crate) type MainResult<T, E = MainError> = Result<T, E>;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub(crate) enum MainError {
    #[snafu(display("Not implemented: {msg}"))]
    NotImplemented { msg: &'static str },
    #[snafu(display("Not implemented yet: {msg}"))]
    NotImplementedYet { msg: &'static str },
    #[snafu(display("CLI error: {source}"))]
    CliParseError { source: clap::Error },
    #[snafu(display("Config error: {source}"))]
    ConfigError { source: config::ConfigError },
    #[snafu(display("RPC Error: {source}"))]
    RpcError { source: solana_client::client_error::ClientError },
    #[snafu(display("Wallet error: {source}"))]
    WalletError { source: wallet::WalletError },
    #[snafu(display("Token error: {source}"))]
    TokenError { source: token::TokenError }
}


struct FormattedMainError { main_error: MainError }
impl Debug for FormattedMainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.main_error)
    }
}
impl Display for FormattedMainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self, f)
    }
}
impl Error for FormattedMainError {}

pub fn lamports_to_sol(lamports: u64) -> f64 {
    lamports as f64 / 1_000_000_000f64
}

pub fn sol_to_lamports(sol: f64) -> u64 {
    (sol * 1_000_000_000f64) as u64
}
