#!/bin/bash

die() { echo "$@" 1>&2 ; exit 1; }

[[ "$1" =~ ^0x[a-fA-F0-9]{40}$ ]] || die "Expecting Ethereum address as sole argument, got '$1'"

declare -a sp_ids=()

for arg in "${@:2}"; do 
	if [[ "$arg" =~ ^[ft]0[0-9]+$ ]]; then
		sp_ids+=("$arg")
	else
		die " Expecting StorageProviderID ( f0xxxx) as next argument, got '$arg'"
	fi
done

ntw="${sp_ids[0]:0:1}"

for spid in "${sp_ids[@]}"; do
	if [[ "${spid:0:1}" != "$ntw" ]]; then
		die "Every Storage Provider ID must belong to the same network"
	fi
done

delegate="$1"

workerkey=$(lotus-miner actor control list | awk '$1 == "worker" {print $3}' | sed 's/\.\.\.$//')

worker_addr=$(lotus wallet list | grep "$workerkey" | awk '{print $1}')

message="$delegate "

for sp_id in "${sp_ids[@]}"; do
	message+=" $sp_id"
done

encoded_hex_message=$(printf "%s" "$message" | od -An -tx1 | tr -d " \n")

signature=$(lotus wallet sign $worker_addr $encoded_hex_message)

printf '{ "signature": "%s", "worker_address": "%s", "message": "%s" }' "$signature" "$worker_addr" "$encoded_hex_message" | curl http://18.116.124.40/filecoin/register -s -XPOST -H 'Content-Type: application/json' -d@/dev/stdin

