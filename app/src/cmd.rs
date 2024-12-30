use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use futures::future::join_all;
use snafu::ResultExt as _;
use solana_client::nonblocking::rpc_client::{self, RpcClient};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    signer::Signer,
    signature::Keypair,
};
use tokio::time::Instant;
use crate::{MainResult, config::{
    self,
    Config,
    KeypairSerde,
    PubkeySerde,
    TestTransferConfig
}, token, worker, wallet, ConfigSnafu, WalletSnafu, TokenSnafu, RpcSnafu, lamports_to_sol, sol_to_lamports, MainError};

pub(crate) struct CmdHandlers {
    pub(crate) config: Config,
    token_owner: Arc<Keypair>,
    token_mint: Arc<Keypair>,
}

impl CmdHandlers {

    pub(crate) fn new(config: Config) -> Self {
        Self {
            token_owner: Arc::new(config.token.owner.clone().0),
            token_mint: Arc::new(config.token.mint.clone().0),
            config,
        }
    }

    pub(crate) fn show_config(&self) -> MainResult<()> {
        println!("{:#?}", self.config);
        Ok(())
    }

    pub(crate) fn connect(&self) -> Arc<RpcClient> {
        Arc::new(RpcClient::new(self.config.rpc.uri.0.to_string()))
    }

    pub(crate) async fn print_sol_balances(&self) -> MainResult<()> {
        let client = self.connect();
        let mut handles= Vec::new();
        // let mut results: Vec<u64> = Vec::new();
        for (i, KeypairSerde(wallet)) in self.config.wallets.0.iter().enumerate() {
            let (pk, client) = (wallet.pubkey(), client.clone());
            handles.push(async move {
                (i, pk, client.get_balance(&pk).await)
            });
        }
        let results = join_all(handles).await;

        for (i, pk, res) in results {
            match res {
                Ok(balance) => println!("{i}. {pk}: {}", crate::lamports_to_sol(balance)),
                Err(err) => println!("{i}. {pk}: error: {err}"),
            }
        }
        Ok(())
    }


    pub(crate) async fn airdrop(&self, sols_amount: f64, confirm: bool) -> MainResult<()> {
        let client = self.connect();
        let lamports = f64::floor(sols_amount * 1_000_000_000f64) as u64;

        let mut handles = Vec::new();
        let mut wallets = self.config.wallets.0
            .iter().enumerate()
            .map(|(i, KeypairSerde(kp))| (format!("{i}. "), kp))
            .collect::<Vec<_>>();
        wallets.push(("token:owner. ".into(), &self.config.token.owner.0));
        for (pfx, wallet) in wallets {
            let (pk, client) = (wallet.pubkey(), client.clone());
            handles.push(async move {
                (pfx, pk, client.request_airdrop(&pk, lamports).await)
            });
        }
        let results = join_all(handles).await;
        if confirm {
            eprintln!("Waiting for confirmation of all transactions...");
            let mut confirmation_handlers = Vec::new();
            for (pfx, pk, res) in results {
                match res {
                    Ok(tx) => {
                        let client = client.clone();
                        confirmation_handlers.push(async move {
                            let res = client.poll_for_signature_with_commitment(&tx, CommitmentConfig::finalized()).await;
                            match res {
                                Ok(_) => println!("{pfx}{pk}: tx id = {tx} - OK"),
                                Err(err) => println!("{pfx}{pk}: tx id = {tx}: error: {err}"),
                            };
                            // (i, pk, tx, res)
                        })
                    },
                    Err(err) => eprintln!("{pfx}{pk}: error: {err}"),
                }
            }
            let _confirmation_results = join_all(confirmation_handlers).await;
            // for (i, pk, tx, res) in confirmation_results {}
        } else {
            for (pfx, pk, res) in results {
                match res {
                    Ok(tx) => println!("{pfx}{pk}: tx id = {tx}"),
                    Err(err) => println!("{pfx}{pk}: error: {err}"),
                }
            }
        }
        Ok(())
    }

    pub(crate) fn print_wallets(&self, pubkey: bool, keypair: bool) -> MainResult<()> {
        for kp in self.config.wallets.0.iter() {
            if pubkey == keypair {
                println!("{} | {}", kp.pubkey(), kp)
            } else {
                match pubkey {
                    true => println!("{}", kp.pubkey()),
                    false => println!("{}", kp),
                }
            }
        }
        Ok(())
    }

    pub(crate) async fn save_wallets_to(&self, save_to: &Path) -> MainResult<()> {
        wallet::save_wallets_to(self.config.wallets.clone(), save_to).await.context(WalletSnafu)
    }

    pub(crate) async fn deploy_token(&self) -> MainResult<()> {
        let client = self.connect();
        let(deploy_tx, _token) = token::deploy(
            client.clone(),
            self.token_mint.clone(),
            self.token_owner.clone(),
        ).await.context(TokenSnafu)?;
        client.poll_for_signature_confirmation(&deploy_tx, 1).await.context(RpcSnafu)?;
        Ok(())
    }

    pub(crate) async fn mint_to(&self, holder: PubkeySerde, amount: f64) -> MainResult<()> {
        let amount = f64::floor(amount * (10f64.powf(token::Token::DECIMALS as f64))) as u64;
        let client = self.connect();
        let token = token::Token::new(client.clone(), self.token_mint.pubkey().clone(), self.token_owner.clone());
        let minting_tx = token.mint_to(&holder.0, amount).await.context(TokenSnafu)?;
        client.poll_for_signature_confirmation(&minting_tx, 1).await.context(RpcSnafu)?;
        Ok(())
    }

    pub(crate) async fn token_balances(&self) -> MainResult<()> {
        let rpc_client = self.connect();
        let token = token::Token::new(
            rpc_client,
            self.config.token.mint.0.pubkey(),
            Arc::new(self.config.token.owner.clone().0)
        );

        let mut handles= Vec::new();
        // let mut results: Vec<u64> = Vec::new();
        for (i, KeypairSerde(wallet)) in self.config.wallets.0.iter().enumerate() {
            let (pk, token) = (wallet.pubkey(), token.clone());
            handles.push(async move {
                (i, pk, token.get_associated_token_account_balance(&pk).await)
            });
        }
        let results = join_all(handles).await;

        for (i, pk, res) in results {
            match res {
                Ok(balance) => println!("{i}. {pk}: {}", token::Token::subunits_to_coins(balance)),
                Err(err) => println!("{i}. {pk}: error: {err}"),
            }
        }
        Ok(())
    }

    pub(crate) async fn test_batched_sols_transfer(&self) -> MainResult<()> {
        let wallets_count = self.config.wallets.0.len();
        if wallets_count < 1 { return Ok(()) }
        let client = self.connect();
        let mut wrk = worker::Worker::new();
        for (i, TestTransferConfig { from, to, amount }) in self.config.test.transfers.sols.clone().into_iter().enumerate() {
            if from >= wallets_count {
                eprintln!("invalid sender wallet index {from}");
                continue
            }
            if to >= wallets_count {
                eprintln!("invalid receiver wallet index {to}");
                continue
            }
            let lamports = sol_to_lamports(amount);
            let amount = lamports_to_sol(lamports);
            let from_kp = self.config.wallets.0[from].clone();
            let to_kp = self.config.wallets.0[to].clone();
            let client = client.clone();
            wrk.push(async move {
                let from_pk = from_kp.pubkey();
                let to_pk = to_kp.pubkey();
                let print_error = |e: &dyn std::error::Error| {
                    eprintln!("{i}. transfer {amount} SOL {from_pk} -> {to_pk} error: {e}")
                };
                let sender_balance = match client.get_balance(&from_pk.0).await {
                    Ok(x) => x, Err(ref e) => return print_error(e),
                };
                if lamports > sender_balance {
                    eprintln!(
                        "{i}. transfer {from_pk} -> {to_pk} error: insufficient balance {} < {}",
                        lamports_to_sol(sender_balance), lamports_to_sol(lamports)
                    );
                }
                let recent_blockhash = match client.get_latest_blockhash().await {
                    Ok(x) => x, Err(ref e) => return print_error(e),
                };
                let transfer_tx = match wallet::transfer_sol(
                    client.as_ref(), &from_kp.0, &to_pk.0, lamports,
                    Some(recent_blockhash),
                    Some(&from_kp.0),
                    Some("Test transfer"),
                ).await { Ok(x) => x, Err(ref e) => return print_error(e)};
                println!("{i}. transferred {amount:.2} from {from_pk} to {to_pk}\n    tx: {transfer_tx}");
                let start_time = Instant::now();
                match client.poll_for_signature_with_commitment(&transfer_tx, CommitmentConfig::confirmed()).await {
                    Ok(x) => x, Err(ref e) => return print_error(e),
                }
                let spent_time = start_time.elapsed();
                println!("{i}. tx: {transfer_tx} confirmed in {spent_time:?}");
                let start_time = Instant::now();
                match client.poll_for_signature_with_commitment(&transfer_tx, CommitmentConfig::finalized()).await {
                    Ok(x) => x, Err(ref e) => return print_error(e),
                }
                let spent_time = start_time.elapsed();
                println!("{i}. tx: {transfer_tx} finalized in {spent_time:?}");
            });
        }
        // if there is a lot of tasks, it would be preferred to use `run` instead of `run_all_joined`
        // because there is a risk to reach some OS limitations on a huge amount of simultaneous connections,
        // especially if validator works on the same machine (I've tested).
        // In other cases `run_all_joined` is possibly faster.
        wrk.run_all_joined().await;
        Ok(())
    }

    pub(crate) async fn test_batched_tokens_transfer(&self) -> MainResult<()> {
        let wallets_count = self.config.wallets.0.len();
        if wallets_count < 1 { return Ok(()) }
        let rpc_client = self.connect();
        let token = token::Token::new(
            rpc_client.clone(),
            self.config.token.mint.pubkey().clone().0,
            Arc::new(self.config.token.owner.clone().0)
        );

        let mut wrk = worker::Worker::new();
        for (i, TestTransferConfig { from, to, amount }) in self.config.test.transfers.tokens.clone().into_iter().enumerate() {
            let token = token.clone();
            if from >= wallets_count {
                eprintln!("invalid sender wallet index {from}");
                continue
            }
            if to >= wallets_count {
                eprintln!("invalid receiver wallet index {to}");
                continue
            }
            let subunits = token::Token::coins_to_subunits(amount);
            let amount = token::Token::subunits_to_coins(subunits);
            let from_kp = self.config.wallets.0[from].clone();
            let to_kp = self.config.wallets.0[to].clone();
            let rpc_client = rpc_client.clone();
            wrk.push(async move {
                let from_pk = from_kp.pubkey();
                let to_pk = to_kp.pubkey();
                let print_error = |e: &dyn std::error::Error| {
                    eprintln!("{i}. transfer {amount} Tokens {from_pk} -> {to_pk} error: {e}")
                };
                let sender_balance = match rpc_client.get_balance(&from_pk.0).await {
                    Ok(x) => x, Err(ref e) => return print_error(e),
                };
                if subunits > sender_balance {
                    eprintln!(
                        "{i}. transfer {from_pk} -> {to_pk} error: insufficient balance {} < {}",
                        lamports_to_sol(sender_balance), lamports_to_sol(subunits)
                    );
                }

                println!("{i}. transferring {amount:.2} from {from_pk} to {to_pk}...");
                let transfer_tx = match token.transfer(&from_kp.0, &to_pk.0, subunits).await {
                    Ok(x) => x, Err(ref e) => return print_error(e)
                };
                println!("{i}. transferred {amount:.2} from {from_pk} to {to_pk}\n    tx: {transfer_tx}");
                let start_time = Instant::now();
                match rpc_client.poll_for_signature_with_commitment(&transfer_tx, CommitmentConfig::confirmed()).await {
                    Ok(x) => x, Err(ref e) => return print_error(e),
                }
                let spent_time = start_time.elapsed();
                println!("{i}. tx: {transfer_tx} confirmed in {spent_time:?}");
                let start_time = Instant::now();
                match rpc_client.poll_for_signature_with_commitment(&transfer_tx, CommitmentConfig::finalized()).await {
                    Ok(x) => x, Err(ref e) => return print_error(e),
                }
                let spent_time = start_time.elapsed();
                println!("{i}. tx: {transfer_tx} finalized in {spent_time:?}");
            });
        }
        // There is no possibility to run it in multithreaded mode,
        // because SPL token client is not Sendable (impl Send).
        // But even a single-threaded performance is enough to send transactions in simultaneous batches.
        // I can make it multithreaded, but it would take some time to rework SPL Token client.
        wrk.run_single_threaded(Some(32)).await;
        Ok(())
    }
}

pub(crate) async fn generate_wallets(count: usize, save_to: Option<PathBuf>) -> MainResult<()> {
    let wallets = config::generate_wallets(count).context(ConfigSnafu)?;
    match save_to {
        Some(save_path_buf) => {
            wallet::save_wallets_to(wallets, save_path_buf.as_path()).await.context(WalletSnafu)
        }
        None => { wallets.print_yaml(); Ok(()) }
    }
}
