use std::error::Error;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use snafu::{ResultExt, Snafu};
use serde::{Serialize, Deserialize, Serializer, Deserializer};
use solana_sdk::{signature::{Keypair, Signer}, bs58};
use solana_sdk::pubkey::Pubkey;
use crate::cli::Cli;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Config {
    pub(crate) rpc: RpcConfig,
    pub(crate) token: TokenConfig,
    pub(crate) test: TestConfig,
    pub(crate) wallets: KeypairList,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RpcConfig {
    pub(crate) uri: Url,
    // TODO: should I implement a rate-limit and a backoff on errors?
    // pub(crate) rate_limit_per_sec: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TokenConfig {
    pub(crate) owner: KeypairSerde,
    pub(crate) mint: KeypairSerde,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TestConfig {
    pub(crate) mint: PubkeySerde,
    pub(crate) transfers: TestTransferCasesConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TestTransferCasesConfig {
    pub(crate) sols: Vec<TestTransferConfig>,
    pub(crate) tokens: Vec<TestTransferConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TestTransferConfig {
    pub(crate) from: usize,
    pub(crate) to: usize,
    pub(crate) amount: f64,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Url(pub(crate) url::Url);
pub(crate) struct KeypairSerde(pub(crate) Keypair);
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct KeypairList(pub(crate) Vec<KeypairSerde>);
#[derive(Clone)]
pub(crate) struct PubkeySerde(pub(crate) Pubkey);

impl Config {
    pub(crate) async fn try_from_cli(cli: &Cli) -> ConfigResult<Self> {
        let config_yaml_file = std::fs::File::open(&cli.config_file).context(ReadFailedSnafu{ path: cli.config_file.clone() })?;
        let config_parse_result = serde_yaml::from_reader::<_, Config>(config_yaml_file);
        let config = config_parse_result.context(ParseFailedSnafu { path: cli.config_file.clone() })?;
        // ... there is a place for re-declaring some of the config values using cli arguments and environment variables
        Ok(config)
    }
}

pub(crate) fn generate_wallets(count: usize) -> ConfigResult<KeypairList> {
    Ok(KeypairList((0..count).map(|_| KeypairSerde(Keypair::new())).collect()))
}

impl core::fmt::Debug for Url {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl PubkeySerde {
    pub(crate) fn to_string(&self) -> String {
        bs58::encode(self.0.to_bytes()).into_string()
    }
}

impl Display for PubkeySerde {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", PubkeySerde::to_string(self))
    }
}

impl core::fmt::Debug for PubkeySerde {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", bs58::encode(self.0.to_bytes()).into_string())
    }
}

impl FromStr for PubkeySerde {
    type Err = Box<dyn Error + Send + Sync + 'static>;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(PubkeySerde(Pubkey::from_str(s)?))
    }
}

impl Serialize for PubkeySerde {
    fn serialize<S: Serializer,>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.to_string().as_str())
    }
}

impl<'de> Deserialize<'de> for PubkeySerde {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let encoded = String::deserialize(deserializer)?;
        Ok(PubkeySerde(Pubkey::from_str(&encoded).map_err(
            |e| serde::de::Error::custom(
                format!("Can't parse PublicKey base58 encoded string: cause: {e}")
            )
        )?))
    }
}

impl KeypairSerde {
    pub(crate) fn pubkey(&self) -> PubkeySerde {
        PubkeySerde(self.0.pubkey())
    }
    pub(crate) fn to_string(&self) -> String {
        bs58::encode(self.0.to_bytes()).into_string()
    }
}

impl Clone for KeypairSerde {
    fn clone(&self) -> Self {
        KeypairSerde(Keypair::from_bytes(self.0.to_bytes().as_ref()).unwrap())
    }
}

impl Display for KeypairSerde {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", KeypairSerde::to_string(self))
    }
}

impl core::fmt::Debug for KeypairSerde {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.pubkey().to_string(), self.to_string())
    }
}

impl Serialize for KeypairSerde {
    fn serialize<S: Serializer,>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.to_string().as_str())
    }
}

impl<'de> Deserialize<'de> for KeypairSerde {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let encoded = String::deserialize(deserializer)?;
        let decoded = bs58::decode(encoded).into_vec().map_err(
            |e| serde::de::Error::custom(
                format!("Can't parse wallet's base58 encoded string: cause: {e}")
            )
        )?;
        Ok(KeypairSerde(
            Keypair::from_bytes(&decoded).map_err(|e| serde::de::Error::custom(
                format!("Can't parse keypair bytes: cause: {e}")
            ))?
        ))
    }
}

impl KeypairList {
    pub(crate) fn print_yaml(&self) {
        for KeypairSerde(kp) in self.0.iter() {
            let kp_base58_encoded = bs58::encode(kp.to_bytes()).into_string();
            println!("- {kp_base58_encoded}");
        }
    }
}


impl core::fmt::Debug for KeypairList {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match f.alternate() {
            true => write!(f, "{:#?}", self.0),
            false => write!(f, "{:?}", self.0),
        }
    }
}

type ConfigResult<T> = Result<T, ConfigError>;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub(crate) enum ConfigError {
    #[snafu(display("Read failed: path: {path}; cause: {source}"))]
    ReadFailed { path: String, source: std::io::Error },
    #[snafu(display("Parse config failed: path: {path}; cause: {source}"))]
    ParseFailed { path: String, source: serde_yaml::Error },
    #[snafu(display("Yaml serialization failed: path: {path}; cause: {source}"))]
    YamlSerializationFailed { path: String, source: serde_yaml::Error },
}
