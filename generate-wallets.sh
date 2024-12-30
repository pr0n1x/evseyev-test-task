#!/bin/sh

here=$(cd "$(dirname "$0")" || exit 111; pwd)

solana-keygen new -s -o "$here/wallets/owner.json" --no-bip39-passphrase --force

for player in 1 2; do
  for wallet in 0 1 2 3 4 5 6 7 8 9; do
      solana-keygen new -s -o "$here/wallets/p${player}w${wallet}.json" --no-bip39-passphrase --force
  done
done
