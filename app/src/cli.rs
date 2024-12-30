use std::path::PathBuf;
use clap::{Parser, Subcommand};
use clap_complete::Shell;
use crate::config::PubkeySerde;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub(crate) struct Cli {
    #[arg(long = "config", short = 'c', value_name = "config", default_value = "env:TEST_TASK_CONFIG_FILE")]
    pub config_file: String,

    
    #[command(subcommand)]
    pub(crate) command: SubCmd,
}


#[derive(Subcommand, Debug)]
pub(crate) enum SubCmd {
    /// Generate shell completion
    Autocompletion {
        #[arg(value_enum)]
        shell: Option<Shell>,
        /// optional command name
        cmd_name: Option<String>,
    },

    /// Wallets management
    Wallet { #[command(subcommand)] command: WalletSubCmd },

    /// Show config values
    ShowConfig,

    /// Show SOL balances
    Balances,

    /// Token management
    Token { #[command(subcommand)] command: TokenSubCmd },

    /// Airdrop
    Airdrop { sols: f64, #[arg(long)] confirm: bool },

    Test { #[command(subcommand)] command: TestSubCmd }
}

#[derive(Subcommand, Debug, Clone)]
pub(crate) enum WalletSubCmd {
    /// Generate wallets (keypairs)
    Generate {
        /// Count of generating keypairs
        count: usize,
        /// Dir to save wallets in solana-cli compatible json format
        save_to: Option<PathBuf>,
    },
    /// List wallets
    List {
        /// show public key (account address)
        #[arg(long)] pubkey: bool,
        /// show keypair
        #[arg(long)] keypair: bool,
    },
    /// Save wallets from the config as solana-cli compatible json files
    Save {
        /// Directory storing wallet json files
        target: PathBuf
    },
    /// Read a keypair json file (solana-cli compatible) and print it's buffer in a base58 encoded string
    Read {
        /// keypair file path
        path: PathBuf,
    }
}

#[derive(Subcommand, Debug, Clone)]
pub(crate) enum TokenSubCmd {
    /// Deploys token (uses config "token.mint" keypair for mint deployment)
    Deploy,
    /// Mints tokens and calculates holder's vault PDA (token account) and send there
    Mint { holder: PubkeySerde, amount: f64 },
    /// Show token balances of all holders (config.wallets)
    Balances,
}

#[derive(Subcommand, Debug, Clone)]
pub(crate) enum TestSubCmd {
    Transfer { #[command(subcommand)] command: TestTransferSubCmd }
}

#[derive(Subcommand, Debug, Clone)]
pub(crate) enum TestTransferSubCmd {
    /// Test batched sols transfer
    Sols,
    /// Test batched tokens transfer
    Tokens,
}
