import os
import subprocess
import json
import pathlib
import pickle
import time

from era_validators import parse_era_validators

# From:
# casper-client put-deploy --chain-name casper-testnet-8 --node-address http://localhost:7777 --secret-key /etc/casper/validator_keys/secret_key.pem --session-path  ./wasm/system_contract_hashes.wasm --payment-amount 1000000000000000
#
# {
#   "jsonrpc": "2.0",
#   "result": {
#     "api_version": "1.0.0",
#     "deploy_hash": "0f963b473cf2c921a19616519f1aabf0907b9bebf42372d0c41540bb280ec0a0"
#   },
#   "id": 1895634799
# }
#
# casper-client query-state --node-address http://localhost:7777 -k $(cat /etc/casper/validator_keys/public_key_hex) -s $(casper-client get-state-root-hash --node-address http://127.0.0.1:7777 | jq -r '.["result"]["state_root_hash"]') | jq -r '.["result"]["stored_value"]["Account"]["named_keys"]["auction"]'
#
# hash-2141636bcf5e15ecced219e53c813b96f99ec8a3bbe31066872b61be49355ce2
AUCTION_HASH = 'hash-0681e58982fc60e93ca415f80327f4b8888435064503672f78b294fc96521aa7'

NODE_ADDRESS = 'http://54.177.84.9:7777'

GET_GLOBAL_STATE_COMMAND = ["casper-client", "get-global-state-hash", "--node-address", NODE_ADDRESS]


def _subprocess_call(command, expect_text) -> dict:
    process = subprocess.Popen(command,
                               stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE)
    stdout, stderr = process.communicate(timeout=30)
    if expect_text.encode('utf-8') not in stdout:
        raise Exception(f"Command: {command}\n {stderr.decode('utf-8')}")
    return json.loads(stdout.decode('utf-8'))


def get_global_state_hash():
    response = _subprocess_call(GET_GLOBAL_STATE_COMMAND, "global_state_hash")
    return response["global_state_hash"]


def get_era_validators(global_state_hash):
    command = ["casper-client", "query-state",
               "--node-address", NODE_ADDRESS,
               "-k", AUCTION_HASH,
               "-s", global_state_hash,
               "-q", "era_validators"]
    response = _subprocess_call(command, "stored_value")
    era_validator_bytes = response["result"]["stored_value"]["CLValue"]["serialized_bytes"]
    return parse_era_validators(era_validator_bytes)


def get_block(block_hash=None):
    command = ["casper-client", "get-block",
               "--node-address", NODE_ADDRESS]
    if block_hash:
        command.append("-b")
        command.append(block_hash)
    return _subprocess_call(command, "block")


def get_all_blocks():
    """
    retrieves all blocks on chain and caches when possible

    will be REALLY slow with large block downloads as calls are throttled.
    """
    cached_blocks_file = pathlib.Path(os.path.realpath(__file__)).parent / "block_cache"
    if pathlib.Path.exists(cached_blocks_file):
        blocks = pickle.load(open(cached_blocks_file, "rb"))
        last_height = blocks[-1]["header"]["height"]
    else:
        blocks = []
        last_height = -1
    block = get_block()["result"]["block"]
    new_blocks = []
    cur_height = block["header"]["height"]
    for _ in range(cur_height - last_height):
        new_blocks.append(block)
        time.sleep(0.1)
        parent_hash = block["header"]["parent_hash"]
        if parent_hash != '0000000000000000000000000000000000000000000000000000000000000000':
            block = get_block(parent_hash)["result"]["block"]

    new_blocks.reverse()
    blocks.extend(new_blocks)
    pickle.dump(blocks, open(cached_blocks_file, "wb"))
    return blocks


# current_global_state_hash = get_global_state_hash()
# print(get_era_validators(current_global_state_hash))
all_blocks = get_all_blocks()
#


def unique_state_root_hashes(blocks):
    pre_srh = ''
    for block in blocks:
        header = block["header"]
        srh = header["state_root_hash"]
        if pre_srh != srh:
            pre_srh = srh
            yield header['era_id'], header['height'], srh


def state_root_hash_by_era(blocks):
    pre_era = ''
    for block in blocks:
        era_id = block["header"]["era_id"]
        if era_id != pre_era:
            pre_era = era_id
            yield era_id, block["header"]["state_root_hash"]


def filtered_era_validators(blocks):
    cached_eras_file = pathlib.Path(os.path.realpath(__file__)).parent / "era_validator_cache"
    if pathlib.Path.exists(cached_eras_file):
        eras = pickle.load(open(cached_eras_file, "rb"))
    else:
        eras = []
    pre_eras = [era[0] for era in eras]
    for era_id, srh in state_root_hash_by_era(all_blocks):
        if era_id in pre_eras:
            continue
        era_val = get_era_validators(srh)
        for era in era_val:
            if era not in eras:
                eras.append(era)
    pickle.dump(eras, open(cached_eras_file, "wb"))
    return eras


def all_validator_keys(era_validators):
    all_keys = set()
    for era in era_validators:
        for key in [validator[0] for validator in era[2]]:
            all_keys.add(key)
    return sorted(list(all_keys))


def save_validator_by_key(era_validators):
    validators = {}
    all_keys = all_validator_keys(era_validators)
    for key in all_keys:
        validators[key] = []
    for era in era_validators:
        cur_vals = era[2]
        keys_in_era = {val[0]: val[1] for val in cur_vals}
        for key in all_keys:
            validators[key] += [keys_in_era.get(key, 0)]
    valid_by_era_path = pathlib.Path(os.path.realpath(__file__)).parent / "validators_by_era.csv"
    with open(valid_by_era_path, "w+") as f:
        f.write(f"era_id,bonded_validator_count,{','.join(all_keys)}\n")
        for era_id in range(len(era_validators)):
            f.write(f"{era_id}")
            # Get count of non-zero bond validators
            bonds = [1 for key in all_keys if validators[key][era_id] > 0]
            f.write(f",{len(bonds)}")
            # Get count of participants
            # TODO - Can we get bid count in this era?
            for key in all_keys:
                f.write(f",{validators[key][era_id]}")
            f.write("\n")


def save_block_info():
    with open("block_proposer.csv", "w+") as f:
        f.write("era_id,height,hash,proposer\n")
        all_blocks = get_all_blocks()
        for block in all_blocks:
            f.write(f'{block["header"]["era_id"]},{block["header"]["height"]},{block["hash"]},{block["header"]["proposer"]}\n')


save_block_info()
era_validators = filtered_era_validators(all_blocks)
save_validator_by_key(era_validators)

# state_root_hash_by_era(all_blocks)

# print(get_era_validators(all_blocks[-1]["header"]["state_root_hash"]))
