#!/usr/bin/env bash

source $NCTL/sh/utils.sh
source $NCTL/sh/contracts/erc20/funcs.sh

unset AMOUNT
unset GAS_PAYMENT
unset GAS_PRICE
unset NET_ID
unset NODE_ID
unset USER_ID

for ARGUMENT in "$@"
do
    KEY=$(echo $ARGUMENT | cut -f1 -d=)
    VALUE=$(echo $ARGUMENT | cut -f2 -d=)
    case "$KEY" in
        amount) AMOUNT=${VALUE} ;;
        gas) GAS_PRICE=${VALUE} ;;
        net) NET_ID=${VALUE} ;;
        node) NODE_ID=${VALUE} ;;
        payment) GAS_PAYMENT=${VALUE} ;;
        user) USER_ID=${VALUE} ;;
        *)
    esac
done

do_erc20_approve \
    ${AMOUNT:-1000000000} \
    ${USER_ID:-1} \
    ${NET_ID:-1} \
    ${NODE_ID:-1} \
    ${GAS_PAYMENT:-$NCTL_DEFAULT_GAS_PAYMENT} \
    ${GAS_PRICE:-$NCTL_DEFAULT_GAS_PRICE}