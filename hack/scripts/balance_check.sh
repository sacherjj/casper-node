#!/bin/bash

set -e

# Modified from Daniel Halford provided script.
# Check input balance
# Requirements: 'apt install jq casper-client'

if [ -z "$1" ] ; then
    echo "Account public key hex not provided."
    exit 1
fi

INPUT_HEX=$1
# can point to server if not run locally
NODE_ADDRESS='http://127.0.0.1:7777'

# -----------------------------------------------------------------------

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
NC='\033[0m'

# 1) Get chain heigh
LFB=$(curl -s "$NODE_ADDRESS/status" | jq -r '.last_added_block_info | .height')

# 2) Get LFB state root hash
LFB_ROOT=$(casper-client get-block  --node-address ${NODE_ADDRESS} -b "$LFB" | jq -r '.result | .block | .header | .state_root_hash')

echo -e "${CYAN}Block ${GREEN}$LFB ${CYAN}state root hash: ${GREEN}$LFB_ROOT${NC}" && echo

# 3) Get purse UREF
PURSE_UREF=$(casper-client query-state --node-address ${NODE_ADDRESS} --key "$INPUT_HEX" --state-root-hash "$LFB_ROOT" | jq -r '.result | .stored_value | .Account | .main_purse')

echo -e "${CYAN}Main purse uref: ${GREEN}$PURSE_UREF${NC}" && echo

# 4) Found balance
BALANCE=$(casper-client get-balance --node-address ${NODE_ADDRESS} --purse-uref "$PURSE_UREF" --state-root-hash "$LFB_ROOT" | jq -r '.result | .balance_value')

echo -e "${CYAN}Input balance: ${GREEN}$BALANCE${NC}" && echo
