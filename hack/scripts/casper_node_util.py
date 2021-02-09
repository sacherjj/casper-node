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
# casper-client query-state --node-address http://3.18.112.103:7777 -k $(cat ~/aws/keys/joe/public_key_hex) -s $(casper-client get-state-root-hash --node-address http://3.18.112.103:7777 | jq -r .result.state_root_hash) | jq .result.stored_value.Account.named_keys
#
# hash-2141636bcf5e15ecced219e53c813b96f99ec8a3bbe31066872b61be49355ce2
AUCTION_HASH = 'hash-71feb0a0853a728317764587db6178887ff751230ac1c524f59f09c9ea53fd7a'

NODE_ADDRESS = 'http://13.56.210.126:7777'
CHAIN_NAME = 'release-test-6'

GET_GLOBAL_STATE_COMMAND = ["casper-client", "get-global-state-hash", "--node-address", NODE_ADDRESS]

CL_NODE_ADDRESSES = ['http://54.177.84.9:7777', 'http://3.16.135.188:7777', 'http://18.144.69.216:7777',
                     'http://13.57.251.65:7777', 'http://3.14.69.138:7777']
OTHER_NODE_ADDRESSES = ['http://34.220.39.73:7777', ]


def _subprocess_call(command, expect_text) -> str:
    process = subprocess.Popen(command,
                               stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE)
    stdout, stderr = process.communicate(timeout=30)
    if expect_text.encode('utf-8') not in stdout:
        raise Exception(f"Command: {command}\n {stderr.decode('utf-8')}")
    return stdout.decode('utf-8')


def _subprocess_call_with_json(command, expect_text) -> dict:
    return json.loads(_subprocess_call(command, expect_text))


def deploy_do_nothing_to_node(node_addr):
    wasm = '/home/sacherjj/repos/casper-node/target/wasm32-unknown-unknown/release/do_nothing.wasm'
    secret_key = '/home/sacherjj/aws/keys/N0/secret_key.pem'
    command = ["casper-client", "put-deploy",
               "--node-address", node_addr,
               "--chain-name", CHAIN_NAME,
               "--secret-key", secret_key,
               "--session-path", wasm,
               "--payment-amount", "10000000000"]
    print(_subprocess_call(command, ''))


def deploy_saved_deploy_to_node(node_addr, deploy_file):
    command = ["casper-client", "send-deploy", "-i", deploy_file, "--node-address", node_addr]
    print(_subprocess_call(command, ''))


def get_global_state_hash():
    response = _subprocess_call_with_json(GET_GLOBAL_STATE_COMMAND, "global_state_hash")
    return response["global_state_hash"]


def get_deploy(deploy_hash: str):
    response = _subprocess_call_with_json(["casper-client", "get-deploy", "--node-address", NODE_ADDRESS, deploy_hash], "result")
    return response


def get_era_validators(global_state_hash):
    command = ["casper-client", "query-state",
               "--node-address", NODE_ADDRESS,
               "-k", AUCTION_HASH,
               "-s", global_state_hash,
               "-q", "era_validators"]
    response = _subprocess_call_with_json(command, "stored_value")
    era_validator_bytes = response["result"]["stored_value"]["CLValue"]["serialized_bytes"]
    return parse_era_validators(era_validator_bytes)


def get_block(block_hash=None):
    command = ["casper-client", "get-block",
               "--node-address", NODE_ADDRESS]
    if block_hash:
        command.append("-b")
        command.append(block_hash)
    return _subprocess_call_with_json(command, "block")


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


def get_all_deploys():
    """
    retrieves all deploys on chain and caches

    will be REALLY slow with large downloads as calls are throttled.
    """
    cached_deploys_file = pathlib.Path(os.path.realpath(__file__)).parent / "deploy_cache"
    if pathlib.Path.exists(cached_deploys_file):
        deploys = pickle.load(open(cached_deploys_file, "rb"))
    else:
        deploys = {}
    for block in get_all_blocks():
        for deploy_hash in block["header"]["deploy_hashes"]:
            if deploy_hash not in deploys.keys():
                deploys[deploy_hash] = get_deploy(deploy_hash)
    pickle.dump(deploys, open(cached_deploys_file, "wb"))
    return deploys

# current_global_state_hash = get_global_state_hash()
# print(get_era_validators(current_global_state_hash))
# all_blocks = get_all_blocks()


#


def unique_state_root_hashes(blocks):
    pre_srh = ''
    for block in blocks:
        header = block["header"]
        srh = header["state_root_hash"]
        if pre_srh != srh:
            pre_srh = srh
            yield header['era_id'], header['height'], srh


def state_root_hash_by_era():
    pre_era = ''
    for block in get_all_blocks():
        era_id = block["header"]["era_id"]
        if era_id != pre_era:
            pre_era = era_id
            yield era_id, block["header"]["state_root_hash"]


def filtered_era_validators():
    cached_eras_file = pathlib.Path(os.path.realpath(__file__)).parent / "era_validator_cache"
    if pathlib.Path.exists(cached_eras_file):
        eras = pickle.load(open(cached_eras_file, "rb"))
    else:
        eras = []
    pre_eras = [era[0] for era in eras]
    for era_id, srh in state_root_hash_by_era(get_all_blocks()):
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
            f.write(
                f'{block["header"]["era_id"]},{block["header"]["height"]},{block["hash"]},{block["header"]["proposer"]}\n')


def get_deploy_hashs_per_block():
    for block in get_all_blocks():
        header = block['header']
        if header['height'] < 999:
            continue
        print(f"{header['era_id']} - {header['height']} - {header['proposer']} - {header['deploy_hashes']}")


def get_proposer_per_era():
    eras = []
    cur_era = -1
    for block in get_all_blocks():
        header = block['header']
        proposer = header['proposer']
        era = header['era_id']
        if cur_era != era:
            cur_era = era
            eras.append(defaultdict(int))
        eras[era][proposer] += 1
    return eras

# save_block_info()
#get_deploy_hashs_per_block()
# for block in get_all_blocks():
#     # print(block)
#     header = block["header"]
#     deploy_count = len(header['deploy_hashes'])
#     for deploy in header['deploy_hashes']:
#         deploy_obj = get_deploy(deploy)
#         print(deploy_obj)
#     transfer_count = len(header['transfer_hashes'])
#     if deploy_count > 2 or transfer_count > 2:
#         print(f"{header['era_id']}-{header['height']} {deploy_count} {transfer_count}")


for deploy in get_all_deploys():
    print(deploy)

era_proposers = get_proposer_per_era()
print(era_proposers)
for era_id, era in enumerate(era_proposers):
    print(era_id, list(era.values()))

# era_validators = filtered_era_validators(all_blocks)
# save_validator_by_key(era_validators)

# state_root_hash_by_era(all_blocks)

# print(get_era_validators(all_blocks[-1]["header"]["state_root_hash"]))

#
# for i in range(10):
#     for node in CL_NODE_ADDRESSES:
#          deploy_do_nothing_to_node(node)
#     time.sleep(65.5)

# for node in CL_NODE_ADDRESSES:
#     deploy_saved_deploy_to_node(node, '~/repos/casper-node/do_nothing_deploy')
