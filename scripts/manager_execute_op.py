#!/usr/bin/env python3
"""
manager_execute_op.py — Execute any authorized vault operation from a
whitelisted guard (strategy) contract, signed by the manager key.

Usage
-----
    python scripts/manager_execute_op.py \\
        --vault   <VAULT_CONTRACT_ID>   \\
        --guard   <STRATEGY_CONTRACT_ID> \\
        --fn      <FUNCTION_NAME>        \\
        --args    <JSON_ARRAY>           \\
        [--dry-run]

Arguments
---------
--vault   Vault contract address (C...)
--guard   Whitelisted strategy/guard contract address (C...)
--fn      Guard function to call, e.g. supply, withdraw_from_lending,
          withdraw_fraction
--args    JSON array of ScVal-encoded arguments forwarded to the guard.
          Each element is one of:
            {"address": "C..."}          → contract address
            {"address": "G..."}          → account address
            {"i128": <int>}              → signed 128-bit integer
            {"i128": {"lo": N, "hi": M}} → i128 split (lo = lower 64 bits)
            {"u32": <int>}               → unsigned 32-bit integer
            {"string": "..."}            → string
            {"symbol": "..."}            → symbol
--dry-run Simulate the transaction without submitting it.
--network mainnet (default) or testnet
--fee     Inclusion fee in stroops (default: 1,000,000 = 0.1 XLM)

Environment
-----------
Set MANAGER_SECRET to the manager's Stellar secret key (S...).
Alternatively pass --secret <S...> on the command line.

Examples
--------
# Supply 0.1 USDC to Blend from the Alpha vault
python scripts/manager_execute_op.py \\
    --vault  CAHHS2EFYMKKBYP3LTUFPDS2JSY4EPQLUDEUOFI5I7P7MOIBSJH5AUKQ \\
    --guard  CBD7QEXZP2RVIEFD4OUWRDAXKB2BM4GKUME3EZQQEBWAABL3IEGXPGKI \\
    --fn     supply \\
    --args   '[{"address":"CAJJZSGMMM3PD7N33TAPHGBUGTB43OC73HVIK2L2G6BNGGGYOSSYBXBD"},
               {"address":"CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"},
               {"i128":1000000}]'

# Withdraw from Blend lending on the Gamma vault
python scripts/manager_execute_op.py \\
    --vault  CCHJFS4OEKTLJLLL6OQXFIS2ECTJRA6WMUNXVS7MD6DKIIB6RTXNBZRW \\
    --guard  CDMPATIFU2P7JRRAQZZ3655IZSNON62V3EUZK2UZH33C7ACQF6EQ2HYM \\
    --fn     withdraw_from_lending \\
    --args   '[{"address":"CAJJZSGMMM3PD7N33TAPHGBUGTB43OC73HVIK2L2G6BNGGGYOSSYBXBD"},
               {"address":"CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"},
               {"i128":1000000}]'

Note: withdraw_fraction is NOT callable via execute_op — it is invoked
directly by the vault during user share redemptions, not by the manager.
"""

import argparse
import json
import os
import sys

from stellar_sdk import Keypair, Network, SorobanServer, TransactionBuilder
from stellar_sdk import scval
from stellar_sdk import xdr as stellar_xdr
from stellar_sdk.soroban_rpc import SendTransactionStatus

# ── Network presets ────────────────────────────────────────────────────────────

NETWORKS = {
    "mainnet": {
        "passphrase": Network.PUBLIC_NETWORK_PASSPHRASE,
        "rpc_url":    "https://mainnet.sorobanrpc.com",
    },
    "testnet": {
        "passphrase": Network.TESTNET_NETWORK_PASSPHRASE,
        "rpc_url":    "https://soroban-testnet.stellar.org",
    },
}

# ── ScVal helpers ──────────────────────────────────────────────────────────────

def _parse_address(raw: str) -> stellar_xdr.ScVal:
    """Return an ScVal Address for either a G-account or C-contract string."""
    return scval.from_address(raw)


def _parse_i128(raw) -> stellar_xdr.ScVal:
    """Accept int, or {"lo": N, "hi": M} split."""
    if isinstance(raw, int):
        return scval.from_int128(raw)
    lo = raw["lo"]
    hi = raw.get("hi", 0)
    value = (hi << 64) | (lo & 0xFFFFFFFFFFFFFFFF)
    if hi < 0 or (hi == 0 and lo < 0):
        # Reconstruct signed value
        value = lo | (hi << 64)
    return scval.from_int128(value)


def parse_scval(obj: dict) -> stellar_xdr.ScVal:
    """
    Convert a JSON object into an stellar_sdk ScVal.

    Supported keys: address, i128, u32, u64, string, symbol, bool, void.
    """
    if "address" in obj:
        return _parse_address(obj["address"])
    if "i128" in obj:
        return _parse_i128(obj["i128"])
    if "u32" in obj:
        return scval.from_uint32(int(obj["u32"]))
    if "u64" in obj:
        return scval.from_uint64(int(obj["u64"]))
    if "string" in obj:
        return scval.from_string(obj["string"])
    if "symbol" in obj:
        return scval.from_symbol(obj["symbol"])
    if "bool" in obj:
        return scval.from_bool(bool(obj["bool"]))
    if "void" in obj:
        return stellar_xdr.ScVal(type=stellar_xdr.ScValType.SCV_VOID)
    raise ValueError(f"Unsupported ScVal type in: {obj}")


def parse_args_array(args_json: str) -> list:
    """Parse the --args JSON array into a list of ScVal objects."""
    items = json.loads(args_json)
    if not isinstance(items, list):
        raise ValueError("--args must be a JSON array")
    return [parse_scval(item) for item in items]


# ── Transaction helpers ────────────────────────────────────────────────────────

def build_execute_op_tx(
    server: SorobanServer,
    manager_kp: Keypair,
    vault_id: str,
    guard_id: str,
    fn_name: str,
    guard_args: list,
    network_passphrase: str,
    inclusion_fee: int,
) -> str:
    """Build and return the XDR of an unsigned execute_op transaction."""
    account = server.load_account(manager_kp.public_key)

    # vault.execute_op(caller, guard, fn_name, args)
    invoke_args = stellar_xdr.InvokeContractArgs(
        contract_address=stellar_xdr.ScAddress(
            type=stellar_xdr.ScAddressType.SC_ADDRESS_TYPE_CONTRACT,
            contract_id=stellar_xdr.Hash(
                _decode_contract(vault_id)
            ),
        ),
        function_name=stellar_xdr.ScSymbol(sc_symbol=b"execute_op"),
        args=[
            scval.from_address(manager_kp.public_key),        # caller (manager G-address)
            scval.from_address(guard_id),                      # guard  (strategy C-address)
            scval.from_symbol(fn_name),                        # fn_name
            scval.from_vec(guard_args),                        # args vec
        ],
    )

    host_fn = stellar_xdr.HostFunction(
        type=stellar_xdr.HostFunctionType.HOST_FUNCTION_TYPE_INVOKE_CONTRACT,
        invoke_contract=invoke_args,
    )

    op = stellar_sdk.InvokeHostFunction(host_function=host_fn, auth=[])

    tx = (
        TransactionBuilder(
            source_account=account,
            network_passphrase=network_passphrase,
            base_fee=inclusion_fee,
        )
        .append_operation(op)
        .set_timeout(30)
        .build()
    )
    return tx.to_xdr()


def _decode_contract(contract_id: str) -> bytes:
    from stellar_sdk import StrKey
    return StrKey.decode_contract(contract_id)


# ── Simulate + submit ──────────────────────────────────────────────────────────

def simulate_and_prepare(server: SorobanServer, tx_xdr: str):
    """
    Simulate the transaction and return the prepared (auth-assembled) XDR.
    Raises on simulation error.
    """
    sim = server.simulate_transaction(tx_xdr)
    if hasattr(sim, "error") and sim.error:
        print("  Simulation failed:")
        print(f"  {sim.error}")
        sys.exit(1)
    # Let the SDK assemble auth and update soroban data
    from stellar_sdk import SorobanDataBuilder
    from stellar_sdk.transaction_envelope import TransactionEnvelope
    te = TransactionEnvelope.from_xdr(tx_xdr, network_passphrase="")
    prepared = server.prepare_transaction(te)
    return prepared


def submit(server: SorobanServer, manager_kp: Keypair, prepared_te, network_passphrase: str):
    """Sign and submit a prepared TransactionEnvelope."""
    prepared_te.sign(manager_kp)
    response = server.send_transaction(prepared_te)
    if response.status == SendTransactionStatus.ERROR:
        print(f"  Submission error: {response.error_result_xdr}")
        sys.exit(1)

    hash_ = response.hash
    print(f"  Tx hash : {hash_}")
    print(f"  Explorer: https://stellar.expert/explorer/public/tx/{hash_}")

    # Poll for confirmation
    import time
    for _ in range(30):
        result = server.get_transaction(hash_)
        from stellar_sdk.soroban_rpc import GetTransactionStatus
        if result.status == GetTransactionStatus.SUCCESS:
            print("  Status  : SUCCESS")
            if result.result_xdr:
                from stellar_sdk import xdr as xdr_
                tx_result = xdr_.TransactionResult.from_xdr(result.result_xdr)
            return result
        if result.status == GetTransactionStatus.FAILED:
            print(f"  Status  : FAILED — {result.result_xdr}")
            sys.exit(1)
        time.sleep(2)
    print("  Timed out waiting for confirmation.")
    sys.exit(1)


# ── Main ───────────────────────────────────────────────────────────────────────

def parse_cli() -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description="Execute an authorized vault operation via the manager key.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    p.add_argument("--vault",    required=True, help="Vault contract address (C...)")
    p.add_argument("--guard",    required=True, help="Whitelisted strategy/guard address (C...)")
    p.add_argument("--fn",       required=True, dest="fn_name", help="Guard function name")
    p.add_argument("--args",     required=True, help='JSON array of ScVal arguments')
    p.add_argument("--network",  default="mainnet", choices=["mainnet", "testnet"])
    p.add_argument("--fee",      type=int, default=1_000_000, help="Inclusion fee in stroops")
    p.add_argument("--secret",   default=None, help="Manager secret key (overrides env)")
    p.add_argument("--dry-run",  action="store_true", help="Simulate only, do not submit")
    return p.parse_args()


def main():
    import stellar_sdk  # local import to give better error if not installed

    args = parse_cli()

    # ── Resolve secret ────────────────────────────────────────────────────────
    secret = args.secret or os.environ.get("MANAGER_SECRET")
    if not secret:
        print("Error: set MANAGER_SECRET env variable or pass --secret <S...>")
        sys.exit(1)
    manager_kp = Keypair.from_secret(secret)

    # ── Network config ────────────────────────────────────────────────────────
    net = NETWORKS[args.network]
    server = SorobanServer(net["rpc_url"])
    passphrase = net["passphrase"]

    # ── Parse guard args ──────────────────────────────────────────────────────
    try:
        guard_args = parse_args_array(args.args)
    except (json.JSONDecodeError, ValueError) as e:
        print(f"Error parsing --args: {e}")
        sys.exit(1)

    print(f"Network : {args.network}")
    print(f"Manager : {manager_kp.public_key}")
    print(f"Vault   : {args.vault}")
    print(f"Guard   : {args.guard}")
    print(f"Fn      : {args.fn_name}")
    print(f"Args    : {args.args}")
    print()

    # ── Build transaction ─────────────────────────────────────────────────────
    account = server.load_account(manager_kp.public_key)

    invoke_args = stellar_xdr.InvokeContractArgs(
        contract_address=stellar_xdr.ScAddress(
            type=stellar_xdr.ScAddressType.SC_ADDRESS_TYPE_CONTRACT,
            contract_id=stellar_xdr.Hash(_decode_contract(args.vault)),
        ),
        function_name=stellar_xdr.ScSymbol(sc_symbol=b"execute_op"),
        args=[
            scval.from_address(manager_kp.public_key),
            scval.from_address(args.guard),
            scval.from_symbol(args.fn_name),
            scval.from_vec(guard_args),
        ],
    )

    host_fn = stellar_xdr.HostFunction(
        type=stellar_xdr.HostFunctionType.HOST_FUNCTION_TYPE_INVOKE_CONTRACT,
        invoke_contract=invoke_args,
    )

    op = stellar_sdk.InvokeHostFunction(host_function=host_fn, auth=[])

    tx = (
        TransactionBuilder(
            source_account=account,
            network_passphrase=passphrase,
            base_fee=args.fee,
        )
        .append_operation(op)
        .set_timeout(30)
        .build()
    )

    # ── Simulate ──────────────────────────────────────────────────────────────
    print("Simulating...")
    sim = server.simulate_transaction(tx)

    if hasattr(sim, "error") and sim.error:
        print(f"Simulation failed:\n{sim.error}")
        sys.exit(1)

    instructions = getattr(sim, "cost", None)
    if instructions:
        print(f"  CPU instructions : {instructions.cpu_insns}")
        print(f"  Memory bytes     : {instructions.mem_bytes}")
    resource_fee = getattr(sim, "min_resource_fee", "?")
    print(f"  Min resource fee : {resource_fee} stroops")

    if args.dry_run:
        print("\nDry-run complete — transaction NOT submitted.")
        return

    # ── Prepare (assembles auth from simulation) ───────────────────────────────
    prepared = server.prepare_transaction(tx)

    # ── Sign & submit ─────────────────────────────────────────────────────────
    print("\nSubmitting...")
    prepared.sign(manager_kp)
    response = server.send_transaction(prepared)

    if response.status == SendTransactionStatus.ERROR:
        print(f"Submission error: {response.error_result_xdr}")
        sys.exit(1)

    hash_ = response.hash
    print(f"  Tx hash : {hash_}")
    if args.network == "mainnet":
        print(f"  Explorer: https://stellar.expert/explorer/public/tx/{hash_}")
    else:
        print(f"  Explorer: https://stellar.expert/explorer/testnet/tx/{hash_}")

    # ── Poll for confirmation ─────────────────────────────────────────────────
    import time
    from stellar_sdk.soroban_rpc import GetTransactionStatus

    print("  Waiting for confirmation", end="", flush=True)
    for _ in range(30):
        result = server.get_transaction(hash_)
        if result.status == GetTransactionStatus.SUCCESS:
            print(" ✓")
            print("  Status  : SUCCESS")
            return
        if result.status == GetTransactionStatus.FAILED:
            print(" ✗")
            print(f"  Status  : FAILED — {result.result_xdr}")
            sys.exit(1)
        print(".", end="", flush=True)
        time.sleep(2)

    print("\nTimed out waiting for ledger confirmation.")
    sys.exit(1)


if __name__ == "__main__":
    main()
