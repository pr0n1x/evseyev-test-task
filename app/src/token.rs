use std::{
    str::FromStr, sync::Arc
};
use snafu::{ResultExt, Snafu};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_request::TokenAccountsFilter;
use solana_sdk::{
    pubkey::{Pubkey, ParsePubkeyError},
    signature::Signature,
    signer::Signer,
};
use spl_token_client::{
    client::{
        ProgramRpcClient,
        ProgramRpcClientSendTransaction,
        RpcClientResponse,
    },
    token::{Token as SplToken, TokenError as SplTokenError},
};
use tokio::sync::Mutex;

#[derive(Clone)]
pub(crate) struct Token {
    pub(crate) rpc_client: Arc<RpcClient>,
    pub(crate) mint: Pubkey,
    pub(crate) owner: Arc<dyn Signer>,
    pub(crate) spl_token: Arc<SplToken<ProgramRpcClientSendTransaction>>
}

pub(crate) async fn deploy(rpc_client: Arc<RpcClient>, mint: Arc<dyn Signer>, owner: Arc<dyn Signer>) -> TokenResult<(Signature, Token)> {
    let token = Token::new(rpc_client, mint.pubkey().clone(), owner.clone());
    let token_owner_pubkey = &owner.pubkey();
    let rpc_client_response = token.spl_token.create_mint(
        token_owner_pubkey,
        Some(token_owner_pubkey),
        Vec::new(),
        &[owner, mint]
    ).await.context(SplTokenSnafu)?;
    Ok((res_tx(rpc_client_response), token))
}

impl Token {
    pub(crate) const DECIMALS: u8 = 6;

    pub(crate) fn new(rpc_client: Arc<RpcClient>, mint: Pubkey, owner: Arc<dyn Signer>) -> Self {
        let token_client = Arc::new(ProgramRpcClient::new(
            rpc_client.clone(), ProgramRpcClientSendTransaction
        ));
        let token_program = spl_token::id();
        Token {
            rpc_client,
            mint,
            owner: Arc::clone(&owner),
            spl_token: Arc::new(SplToken::new(
                token_client,
                &token_program,
                &mint,
                Some(Self::DECIMALS),
                owner.clone()
            ))
        }
    }

    pub(crate) fn coins_to_subunits(amount: f64) -> u64 {
        (amount * 10f64.powi(Self::DECIMALS as i32)).floor() as u64
    }

    pub(crate) fn subunits_to_coins(subunits: u64) -> f64 {
        subunits as f64 / 10f64.powi(Self::DECIMALS as i32)
    }

    pub(crate) async fn mint_to(&self, dest_holder: &Pubkey, amount: u64) -> TokenResult<Signature> {
        // let rpc_client_response = self.spl_token.mint_to(
        //     dest_token_account, &self.owner.pubkey(),
        //     amount,
        //     &[self.owner.clone()],
        // ).await?;
        // Ok(res_tx(rpc_client_response))
        todo!("Implement Token::mint_to")
    }

    pub(crate) async fn get_token_account_balance(&self, token_account: &Pubkey) -> TokenResult<u64> {
        let sender_ui_balance = self.rpc_client.get_token_account_balance(&token_account)
            .await.context(TokenRpcSnafu)?;
        Ok(Self::coins_to_subunits(
            sender_ui_balance.ui_amount
                .ok_or(TokenError::Unexpected {
                    msg: "token account balance response value is empty".to_string(),
                })?,
        ))
    }

    pub(crate) async fn get_associated_token_account_balance(&self, holder: &Pubkey) -> TokenResult<u64> {
        self.get_token_account_balance(
            &self.spl_token.get_associated_token_address(holder),
        ).await
    }

    pub(crate) async fn get_accumulated_balance(&self, holder: &Pubkey) -> TokenResult<AccumulatedTokenBalance> {
        let token_accounts = self.rpc_client.get_token_accounts_by_owner(
            holder, TokenAccountsFilter::Mint(self.mint.clone()),
        ).await.context(TokenRpcSnafu)?;
        let token_accounts = token_accounts.into_iter()
            .map(|x| Pubkey::from_str(&x.pubkey))
            .collect::<Result<Vec<Pubkey>, ParsePubkeyError>>()
            .context(ParsePubkeySnafu)?;
        let mut accum = AccumulatedTokenBalance{ sum: 0, details: Vec::new() };
        for ta in token_accounts {
            let balance = self.get_token_account_balance(&ta).await?;
            accum.sum += balance;
            accum.details.push((ta, balance))
        }
        Ok(accum)
    }

    pub(crate) async fn create_token_account(
        &self,
        holder: &(dyn Signer + Sync),
        token_account: &(dyn Signer + Sync),
    ) -> TokenResult<Signature> {
        todo!("Implement Token::create_ta")
    }

    pub(crate) async fn create_associated_token_account(&self, holder: &(dyn Signer + Sync)) -> TokenResult<Signature> {
        todo!("Implement Token::create_ata")
    }

    pub(crate) async fn transfer_between_token_accounts(
        &self,
        sender: &(dyn Signer + Sync),
        source_ta: &Pubkey,
        destination_ta: &Pubkey,
        subunits: u64,
    ) -> TokenResult<Signature> {
        let sender_pk = sender.pubkey();
        // TODO: check source token account belongs to sender
        if self.get_token_account_balance(source_ta).await? < subunits {
            return Err(TokenError::InsufficientBalance);
        }
        Ok(res_tx(self.spl_token.transfer(
            &source_ta,
            &destination_ta,
            &sender_pk,
            subunits,
            &[sender],
        ).await.context(SplTokenSnafu)?))
    }

    pub(crate) async fn transfer(
        &self,
        sender: &(dyn Signer + Sync),
        receiver: &Pubkey,
        subunits: u64,
    ) -> TokenResult<Signature> {
        self.transfer_between_token_accounts(
            sender,
            &self.spl_token.get_associated_token_address(&sender.pubkey()),
            &self.spl_token.get_associated_token_address(receiver),
            subunits
        ).await
    }
}

pub(crate) struct AccumulatedTokenBalance {
    pub(crate) sum: u64,
    pub(crate) details: Vec<(Pubkey, u64)>,
}

fn res_tx(response: RpcClientResponse) -> Signature {
    match response {
        RpcClientResponse::Signature(x) => x,
        _ => unreachable!("using ProgramRpcClientSendTransaction result always have to be Signature"),
    }
}

pub(crate) type TokenResult<T> = Result<T, TokenError>;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub(crate) enum TokenError {
    #[snafu(display("Unexpected token error: {msg}"))]
    Unexpected { msg: String },
    #[snafu(display("SPL token error: {source}"))]
    SplTokenError { source: SplTokenError },
    #[snafu(display("RPC error: {source}"))]
    TokenRpcError { source: solana_client::client_error::ClientError },
    #[snafu(display("Insufficient token balance"))]
    InsufficientBalance,
    #[snafu(display("{source}"))]
    ParsePubkeyError { source: ParsePubkeyError },
}
