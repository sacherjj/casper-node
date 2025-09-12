#!/usr/bin/env bash

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)"
GENESIS_DIR="$ROOT_DIR/target/genesis"
CONFIG_DIR="$ROOT_DIR/target/config"

echo "Checked out Github hash $CURRENT_HASH"

LATEST_HASH=$(curl -s https://genesis.casper.network/dev-net/latest_git_hash | tr -d '\n')
echo "Latest Hash from dev-net protocol is $LATEST_HASH"

if [ "$CURRENT_HASH" != "$LATEST_HASH" ]; then
	  echo "Last published dev-net protocol has same hash, erroring out."
	  exit 1 # This fails job and stops workflow
fi

LATEST_PROTOCOL_VERSION="$(curl -s https://genesis.casper.network/dev-net/protocol_versions | tail -n 1 | tr -d '\n')"

IFS="_"
# Read latest protocol parts into array
read -ra LPVA <<< "$LATEST_PROTOCOL_VERSION"

# Incrementing one to patch
NEW_PROTOCOL_VERSION=${LVPA[0]}_${LVPA[1]}_$((${LVPA[2]} + 1))
PROTOCOL_DIR="$GENESIS_DIR/$NEW_PROTOCOL_VERSION"
mkdir -p PROTOCOL_DIR
echo $NEW_PROTOCOL_VERSION >> "$GENESIS_DIR/protocol_versions"
echo $CURRENT_HASH >> "$GENESIS_DIR/latest_git_hash"

mkdir -p "$CONFIG_DIR"
cd "$ROOT_DIR" || exit 1
curl -JLO "http://genesis.casper.network/artifacts/casper-node/$LATEST_HASH/bin.tar.gz"
mv bin.tar.gz "$PROTOCOL_DIR/"
curl -JLO "http://genesis.casper.network/artifacts/casper-node/$LATEST_HASH/config.tar.gz"
cd config || exit 1
# This will validate that files retrieved were good. Should error if just curl output
tar -xzvf ../config.tar.gz .

ls -alr "$ROOT_DIR"
