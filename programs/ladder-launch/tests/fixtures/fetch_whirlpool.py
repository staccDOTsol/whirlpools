"""Download the mainnet Whirlpool program binary and the launchpad config accounts used by the tests.
Usage: RPC_URL=https://... python3 tests/fixtures/fetch_whirlpool.py   (run from programs/ladder-launch)"""
import base64, json, os, urllib.request
RPC = os.environ["RPC_URL"]
def call(method, params):
    req = urllib.request.Request(RPC, data=json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode(), headers={"Content-Type": "application/json"})
    return json.load(urllib.request.urlopen(req, timeout=120))["result"]
def account(addr):
    v = call("getAccountInfo", [addr, {"encoding": "base64"}])["value"]
    return v, base64.b64decode(v["data"][0])
here = os.path.dirname(os.path.abspath(__file__))
_, prog = account("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc")
assert prog[:4] == b"\x02\x00\x00\x00", "expected an upgradeable program account"
# programdata address is the 32 bytes after the 4-byte enum tag; encode without a base58 dependency via RPC lookup
pd_bytes = prog[4:36]
ALPHABET = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
def b58(b):
    n = int.from_bytes(b, "big"); out = bytearray()
    while n: n, r = divmod(n, 58); out.append(ALPHABET[r])
    pad = len(b) - len(b.lstrip(b"\0")); return (ALPHABET[0:1] * pad + bytes(reversed(out))).decode()
_, pdd = account(b58(pd_bytes))
open(os.path.join(here, "whirlpool.so"), "wb").write(pdd[45:])  # UpgradeableLoaderState::ProgramData header
for name, addr in (("whirlpools_config", "12yTE48QR6bGK4EMcyY8XsARbX1TRTEbwHYSuuxR1Hp8"), ("adaptive_fee_tier", "6assHYd5438D91RXfMUrETNqGHmcbfzFCJsBjHmbkeu9")):
    v, _ = account(addr)
    json.dump({"pubkey": addr, "account": {"lamports": v["lamports"], "data": [v["data"][0], "base64"], "owner": v["owner"], "executable": False, "rentEpoch": 0}}, open(os.path.join(here, f"{name}.json"), "w"))
print("fixtures written")
