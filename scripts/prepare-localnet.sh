#!/bin/bash
set -e

workdir=${WORKDIR:-"$(cd "$(dirname "$0")/.."; pwd)/workdir"}
rpc_uri="http://localhost:8899"
owner_kp="$workdir/owner.json"
owner_pk="$(solana-keygen pubkey "$owner_kp")"
mint_kp="$workdir/mint.json"
mint_pk="$(solana-keygen pubkey "$mint_kp")"
decimals=6
sol_airdrop_amount=1000
mint_amount=1000

echo ""
echo "Airdrop $sol_airdrop_amount for token owner ($owner_pk)"
solana -u "$rpc_uri" -k "$owner_kp" airdrop "$sol_airdrop_amount"
echo ""
echo "Create token $mint_pk"
spl-token -u "$rpc_uri" create-token --decimals "$decimals" --fee-payer "$owner_kp" --mint-authority "$owner_pk" -- "$mint_kp"
echo ""
echo "Create owner's token account"
spl-token -u "$rpc_uri" create-account "$mint_pk" --owner "$owner_pk" --fee-payer "$owner_kp"

find "$workdir/wallets" -type f -name "*.json" | sort | while read -r keypair; do
  keypair_filename="$(basename "$keypair")"
  pubkey="$(solana-keygen pubkey "$keypair")"
  echo ""
  echo "Airdrop $sol_airdrop_amount SOL for $pubkey ($keypair_filename)"
  solana -u "$rpc_uri" -k "$keypair" airdrop "$sol_airdrop_amount"
  echo ""
  echo "Create token account for $pubkey"
  spl-token -u "$rpc_uri" create-account "$mint_pk" --owner "$pubkey" --fee-payer "$keypair"
  associated_token_account="$(spl-token address -v --token "$mint_pk" --owner "$pubkey" | tail -n2 | xargs | sed 's/Associated token address: //g')"
  echo ""
  echo "Mint $mint_amount tokens for $pubkey ($keypair_filename)"
  spl-token -u "$rpc_uri" mint "$mint_pk" "$mint_amount" --fee-payer "$keypair" --mint-authority "$owner_kp" "$associated_token_account"
done
