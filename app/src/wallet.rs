use std::io::Write;
use std::path::Path;
use snafu::{ResultExt, Snafu};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    bs58,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::Signature,
    signer::Signer,
    system_instruction,
    transaction::Transaction,
};
use crate::config::{KeypairList, KeypairSerde};

pub(crate) async fn save_wallets_to(wallets: KeypairList, save_to: &Path) -> WalletResult<()> {
    let save_path_to_str = save_to.to_string_lossy();
    if !std::fs::metadata(save_to)
        .context(SaveJsonWalletToFileSnafu { path: save_path_to_str.to_string() })?
        .is_dir()
    {
        return Err(WalletError::InvalidWalletSaveDir {
            path: save_path_to_str.to_string()
        });
    }
    for (i, KeypairSerde(kp)) in wallets.0.iter().enumerate() {
        let kp_bytes = kp.to_bytes();
        let wallet_json = serde_json::to_string(kp_bytes.as_slice())
            .context(SerializeWalletIntoJsonSnafu)?;
        let wallet_file_path_buf = save_to.join(format!("id{i:06}.json"));
        let wallet_file_path_string = wallet_file_path_buf.to_string_lossy().to_string();
        let save_error_ctx = SaveJsonWalletToFileSnafu { path: wallet_file_path_string.clone() };
        let mut wallet_file = std::fs::File::create(wallet_file_path_buf)
            .context(save_error_ctx.clone())?;
        wallet_file.write_all(wallet_json.as_bytes()).context(save_error_ctx)?;
        let kp_base58_encoded = bs58::encode(kp_bytes).into_string();
        println!("- keypair: {kp_base58_encoded}\n  saved_to: {wallet_file_path_string}");
    }
    Ok(())
}

pub(crate) async fn convert_keypair_file_to_base58_string(wallet_path: &Path) -> WalletResult<String> {
    let wallet_file = std::fs::File::open(wallet_path)
        .context(ReadJsonWalletFileSnafu { path: wallet_path.to_string_lossy() })?;
    let kp_bytes: Vec<u8> = serde_json::from_reader(wallet_file)
        .context(ParseJsonWalletFileSnafu { path: wallet_path.to_string_lossy() })?;
    Ok(bs58::encode(kp_bytes).into_string())
}

pub(crate) async fn transfer_sol(
    rpc_client: &RpcClient,
    sender: &(dyn Signer + Sync),
    receiver: &Pubkey,
    lamports: u64,
    recent_blockhash: Option<solana_sdk::hash::Hash>,
    payer: Option<&(dyn Signer + Sync)>,
    memo: Option<impl AsRef<str>>,
) -> WalletResult<Signature> {
    let sender_pk = &sender.pubkey();
    let mut instructions = [
        system_instruction::transfer(sender_pk, receiver, lamports),
    ].into_iter().collect::<Vec<_>>();
    let instructions = instructions.with_memo(memo);
    let recent_blockhash = match recent_blockhash {
        Some(x) => x,
        None => rpc_client.get_latest_blockhash().await.context(WalletRpcSnafu)?,
    };

    let payer_pk = payer.map(|kp| kp.pubkey());
    let tx = Transaction::new_signed_with_payer(
        &instructions, payer_pk.as_ref(), &[sender], recent_blockhash
    );
    rpc_client.send_transaction(&tx).await.context(WalletRpcSnafu)
}

pub(crate) type WalletResult<T> = Result<T, WalletError>;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub(crate) enum WalletError {
    #[snafu(display("Not implemented yet: {msg}"))]
    NotImplementedYet { msg: &'static str },
    #[snafu(display("RPC Error: {source}"))]
    WalletRpcError { source: solana_client::client_error::ClientError },
    #[snafu(display("Invalid wallet save dir: {path}"))]
    InvalidWalletSaveDir { path: String },
    #[snafu(display("Can't serialize the generated wallet into json format: {source}"))]
    SerializeWalletIntoJsonError { source: serde_json::Error },
    #[snafu(display("Can't save generated wallet to file: path: {path}; cause: {source}"))]
    SaveJsonWalletToFileError { path: String, source: std::io::Error },
    #[snafu(display("Can't read keypair json file: path: {path}; cause: {source}"))]
    ReadJsonWalletFileError { path: String, source: std::io::Error },
    #[snafu(display("Can't parse keypair json file: path: {path}; cause: {source}"))]
    ParseJsonWalletFileError { path: String, source: serde_json::Error },
    ProgramError { source: solana_sdk::program_error::ProgramError },
}

pub trait WithMemo {
    fn with_memo(self, memo: Option<impl AsRef<str>>) -> Self;
}

impl WithMemo for Vec<Instruction> {
    fn with_memo(mut self, memo: Option<impl AsRef<str>>) -> Self {
        if let Some(memo) = &memo {
            let memo = memo.as_ref();
            let memo_ix = Instruction {
                program_id: Pubkey::from(spl_memo::id().to_bytes()),
                accounts: vec![],
                data: memo.as_bytes().to_vec(),
            };
            self.push(memo_ix);
        }
        self
    }
}
