# ladder-launch

Launchpad on Orca Whirlpools, written in Pinocchio. Buyers do not buy: they deposit a quote
token and the program pairs it with tokens from a fixed reserve into a concentrated position
that is ~90% token / ~10% quote by value (lower bound ~21% under spot, open top). The launch
PDA owns the Whirlpool position; the depositor gets a plain SPL NFT that is the seat's only key.

* Exits use the seed-clawback rule: the holder takes the quote side and fees, seeded tokens
  return to the reserve, only tokens beyond the seed are theirs.
* Exits are refused for `min_age_s` after entry and rate-limited to `exit_cap_bps` of the
  launch's liquidity per minute (the first exit in a window always passes).
* A slice of the reserve (`floor_bps`) sits in a permanently locked token-only position so
  there is always an ask; its fees split creator/treasury.
* Any quote works: SPL Token or Token-2022 without transfer hooks, permanent delegates, close
  authority, default-frozen state, confidential transfer, non-transferability or pausing.
  Either mint ordering is handled.
* Pools open on WhirlpoolsConfig `12yTE48QR6bGK4EMcyY8XsARbX1TRTEbwHYSuuxR1Hp8` (25% protocol
  fee) with the adaptive fee tier `6assHYd5438D91RXfMUrETNqGHmcbfzFCJsBjHmbkeu9`
  (tick spacing 128, 1% base). Tick arrays are dynamic and created lazily by deposits.

Instructions (tag byte first): `0 create_launch`, `1 init_pool`, `2 new_bundle`, `3 seed_floor`,
`4 deposit`, `5 exit`, `6 collect_fees`, `7 collect_floor_fees`. Account lists are documented
at the top of each file in `src/instructions/`.

## Build and test

```
cargo build-sbf
RPC_URL=https://<mainnet rpc> python3 tests/fixtures/fetch_whirlpool.py   # once
cargo test
```

The integration tests run the whole lifecycle on litesvm against the mainnet Whirlpool binary,
for a WSOL quote (token as Whirlpool A) and a Token-2022 quote (token as B).
