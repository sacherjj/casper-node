#!/usr/bin/env bash
set -e

cd /etc/casper

# This will pull latest genesis files down into current directory.
# The expectation is this is installed in and run in /etc/casper with sudo

BRANCH_NAME="master"

BASE_PATH="https://raw.githubusercontent.com/CasperLabs/casper-node/${BRANCH_NAME}/resources/production"
ACCOUNTS_CSV_PATH="${BASE_PATH}/accounts.csv"
CHAINSPEC_TOML_PATH="${BASE_PATH}/chainspec.toml"
VALIDATION_PATH="${BASE_PATH}/validation.md5"

files=("accounts.csv" "chainspec.toml" "validation.md5")
for file in "${files[@]}"; do
  if [[ -f $file ]]; then
    echo "deleting old $file."
    rm "$file"
  fi
done

curl -sLJO $ACCOUNTS_CSV_PATH
curl -sLJO $CHAINSPEC_TOML_PATH
curl -sLJO $VALIDATION_PATH

md5sum -c ./validation.md5
