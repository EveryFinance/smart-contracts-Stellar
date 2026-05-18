#!/usr/bin/env python3
"""
Build and submit Blend supply + withdraw transactions for each vault on mainnet.

authorize_as_current_contract is blocked in the Soroban host's recording-auth
simulation mode (used by stellar contract invoke / simulateTransaction).
Strategy: simulate first to capture the partial footprint, then manually add
the missing ledger keys (SAC balance entries for strategy / pool), set padded
resource limits, and submit without re-simulating.
"""

import os
import sys
import time
import base64
import json

from stellar_sdk import (
    Keypair, Network, SorobanServer, TransactionBuilder, StrKey
)
from stellar_sdk import scval
from stellar_sdk import xdr as stellar_xdr

# ── Config ────────────────────────────────────────────────────────────────────
MANAGER_SECRET = os.environ.get("MANAGER_SECRET", "")
NETWORK_PASSPHRASE = Network.PUBLIC_NETWORK_PASSPHRASE
RPC_URL = "https://mainnet.sorobanrpc.com"
BASE_FEE = 10_000_000  # 1 XLM max fee (generous)

BLEND_POOL = "CAJJZSGMMM3PD7N33TAPHGBUGTB43OC73HVIK2L2G6BNGGGYOSSYBXBD"
USDC_ID    = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"

# New (fixed) Blend strategy contracts deployed 2026-05-18
VAULTS = {
    "alpha": {
        "vault":       "CAHHS2EFYMKKBYP3LTUFPDS2JSY4EPQLUDEUOFI5I7P7MOIBSJH5AUKQ",
        "blend_new":   "CBD7QEXZP2RVIEFD4OUWRDAXKB2BM4GKUME3EZQQEBWAABL3IEGXPGKI",
        "supply_amount": 1_000_000,   # 0.1 USDC (7 decimals)
    },
    "beta": {
        "vault":       "CDYB5FK54OXV36AQ2TBK6V2K6KYN6RXNIID6HMCYUP7EJ4BEV7BAVIYB",
        "blend_new":   "CBO5XSLPO4DCJJSWWWCPHZ6JDFKPFBPMQDLJWUCJJ3PEWRYO7V6JOJ7V",
        "supply_amount": 1_000_000,
    },
    "gamma": {
        "vault":       "CCHJFS4OEKTLJLLL6OQXFIS2ECTJRA6WMUNXVS7MD6DKIIB6RTXNBZRW",
        "blend_new":   "CDMPATIFU2P7JRRAQZZ3655IZSNON62V3EUZK2UZH33C7ACQF6EQ2HYM",
        "supply_amount": 1_000_000,
    },
}

# ── Helpers ───────────────────────────────────────────────────────────────────

def contract_sc_address(contract_id: str) -> stellar_xdr.ScVal:
    """Return ScVal for a contract address."""
    return scval.from_address(StrKey.encode_contract(StrKey.decode_contract(contract_id)))


def contract_account_address(account_id: str) -> stellar_xdr.ScVal:
    """Return ScVal for a G-address."""
    return scval.from_address(account_id)


def build_sac_balance_key(token_contract: str, holder: str) -> stellar_xdr.LedgerKey:
    """
    Build the ledger key for a SAC (Stellar Asset Contract) token balance entry.
    SAC stores balances as ContractData with:
      key = Map { "Balance": Address(holder) }
    """
    token_bytes = StrKey.decode_contract(token_contract)
    holder_is_contract = holder.startswith("C")

    if holder_is_contract:
        holder_bytes = StrKey.decode_contract(holder)
        sc_holder = stellar_xdr.ScVal(
            type=stellar_xdr.ScValType.SCV_ADDRESS,
            address=stellar_xdr.ScAddress(
                type=stellar_xdr.ScAddressType.SC_ADDRESS_TYPE_CONTRACT,
                contract_id=stellar_xdr.Hash(holder_bytes),
            ),
        )
    else:
        from stellar_sdk import Keypair as _KP
        kp = _KP.from_public_key(holder)
        sc_holder = stellar_xdr.ScVal(
            type=stellar_xdr.ScValType.SCV_ADDRESS,
            address=stellar_xdr.ScAddress(
                type=stellar_xdr.ScAddressType.SC_ADDRESS_TYPE_ACCOUNT,
                account_id=stellar_xdr.AccountID(
                    account_id=stellar_xdr.PublicKey(
                        type=stellar_xdr.PublicKeyType.PUBLIC_KEY_TYPE_ED25519,
                        ed25519=stellar_xdr.Uint256(kp.raw_public_key()),
                    )
                ),
            ),
        )

    balance_key = stellar_xdr.ScVal(
        type=stellar_xdr.ScValType.SCV_VEC,
        vec=stellar_xdr.ScVec(
            sc_vec=[
                stellar_xdr.ScVal(
                    type=stellar_xdr.ScValType.SCV_SYMBOL,
                    sym=stellar_xdr.ScSymbol(sc_symbol=b"Balance"),
                ),
                sc_holder,
            ]
        ),
    )

    return stellar_xdr.LedgerKey(
        type=stellar_xdr.LedgerEntryType.CONTRACT_DATA,
        contract_data=stellar_xdr.LedgerKeyContractData(
            contract=stellar_xdr.ScAddress(
                type=stellar_xdr.ScAddressType.SC_ADDRESS_TYPE_CONTRACT,
                contract_id=stellar_xdr.Hash(token_bytes),
            ),
            key=balance_key,
            durability=stellar_xdr.ContractDataDurability.PERSISTENT,
        ),
    )


def build_contract_instance_key(contract_id: str) -> stellar_xdr.LedgerKey:
    """Build ledger key for a contract's instance storage."""
    contract_bytes = StrKey.decode_contract(contract_id)
    return stellar_xdr.LedgerKey(
        type=stellar_xdr.LedgerEntryType.CONTRACT_DATA,
        contract_data=stellar_xdr.LedgerKeyContractData(
            contract=stellar_xdr.ScAddress(
                type=stellar_xdr.ScAddressType.SC_ADDRESS_TYPE_CONTRACT,
                contract_id=stellar_xdr.Hash(contract_bytes),
            ),
            key=stellar_xdr.ScVal(
                type=stellar_xdr.ScValType.SCV_LEDGER_KEY_CONTRACT_INSTANCE
            ),
            durability=stellar_xdr.ContractDataDurability.PERSISTENT,
        ),
    )


def build_execute_op_invoke(
    vault_id: str,
    blend_id: str,
    manager_addr: str,
    pool: str,
    usdc: str,
    amount: int,
) -> stellar_xdr.HostFunction:
    """Build the InvokeContract HostFunction for vault.execute_op(...)."""
    manager_sc = scval.from_address(manager_addr)
    vault_sc   = scval.from_address(StrKey.encode_contract(StrKey.decode_contract(vault_id)))
    blend_sc   = scval.from_address(StrKey.encode_contract(StrKey.decode_contract(blend_id)))

    # args to "supply": [pool, asset, amount] (what vault passes to strategy)
    pool_sc   = scval.from_address(StrKey.encode_contract(StrKey.decode_contract(pool)))
    usdc_sc   = scval.from_address(StrKey.encode_contract(StrKey.decode_contract(usdc)))
    amount_sc = scval.from_int128(amount)

    inner_args = scval.from_vec([pool_sc, usdc_sc, amount_sc])

    invoke_args = stellar_xdr.InvokeContractArgs(
        contract_address=stellar_xdr.ScAddress(
            type=stellar_xdr.ScAddressType.SC_ADDRESS_TYPE_CONTRACT,
            contract_id=stellar_xdr.Hash(StrKey.decode_contract(vault_id)),
        ),
        function_name=stellar_xdr.ScSymbol(sc_symbol=b"execute_op"),
        args=[
            manager_sc,
            blend_sc,
            scval.from_symbol("supply"),
            inner_args,
        ],
    )

    return stellar_xdr.HostFunction(
        type=stellar_xdr.HostFunctionType.HOST_FUNCTION_TYPE_INVOKE_CONTRACT,
        invoke_contract=invoke_args,
    )


def build_withdraw_invoke(
    vault_id: str,
    blend_id: str,
    manager_addr: str,
    pool: str,
    usdc: str,
    amount: int,
) -> stellar_xdr.HostFunction:
    """Build InvokeContract HostFunction for vault.execute_op → blend.withdraw_from_lending."""
    manager_sc = scval.from_address(manager_addr)
    blend_sc   = scval.from_address(StrKey.encode_contract(StrKey.decode_contract(blend_id)))
    pool_sc    = scval.from_address(StrKey.encode_contract(StrKey.decode_contract(pool)))
    usdc_sc    = scval.from_address(StrKey.encode_contract(StrKey.decode_contract(usdc)))
    amount_sc  = scval.from_int128(amount)

    inner_args = scval.from_vec([pool_sc, usdc_sc, amount_sc])

    invoke_args = stellar_xdr.InvokeContractArgs(
        contract_address=stellar_xdr.ScAddress(
            type=stellar_xdr.ScAddressType.SC_ADDRESS_TYPE_CONTRACT,
            contract_id=stellar_xdr.Hash(StrKey.decode_contract(vault_id)),
        ),
        function_name=stellar_xdr.ScSymbol(sc_symbol=b"execute_op"),
        args=[
            manager_sc,
            blend_sc,
            scval.from_symbol("withdraw_from_lending"),
            inner_args,
        ],
    )
    return stellar_xdr.HostFunction(
        type=stellar_xdr.HostFunctionType.HOST_FUNCTION_TYPE_INVOKE_CONTRACT,
        invoke_contract=invoke_args,
    )


def try_simulate(server: SorobanServer, tx_xdr: str) -> dict:
    """Call simulateTransaction and return the raw result dict."""
    result = server.simulate_transaction(tx_xdr)
    return result


def build_padded_soroban_data(
    footprint_keys: list,  # list of LedgerKey XDR strings (base64)
    rw_keys: list,         # subset that are read-write
) -> stellar_xdr.SorobanTransactionData:
    """
    Build SorobanTransactionData with generous resource padding.
    footprint_keys = all keys (read-only + read-write) as base64 XDR strings.
    rw_keys = keys that will be written, as base64 XDR strings.
    """
    ro_keys = [k for k in footprint_keys if k not in rw_keys]

    ro_ledger_keys = [
        stellar_xdr.LedgerKey.from_xdr(k) for k in ro_keys
    ]
    rw_ledger_keys = [
        stellar_xdr.LedgerKey.from_xdr(k) for k in rw_keys
    ]

    footprint = stellar_xdr.LedgerFootprint(
        read_only=ro_ledger_keys,
        read_write=rw_ledger_keys,
    )
    resources = stellar_xdr.SorobanResources(
        footprint=footprint,
        instructions=20_000_000,    # 20M instructions (generous)
        read_bytes=100_000,
        write_bytes=10_000,
    )
    ext = stellar_xdr.SorobanTransactionDataExt(v=0)
    return stellar_xdr.SorobanTransactionData(
        ext=ext,
        resource_fee=stellar_xdr.Int64(1_000_000),  # 0.1 XLM resource fee
        resources=resources,
    )


def get_manager_auth_entry(
    server: SorobanServer,
    manager_kp: Keypair,
    vault_id: str,
    blend_id: str,
    fn_name_str: str,
    inner_args_sc: stellar_xdr.ScVal,
    expiration_ledger: int,
) -> stellar_xdr.SorobanAuthorizationEntry:
    """
    Build + sign a SorobanAuthorizationEntry for the manager authorizing
    vault.execute_op(manager, blend, fn_name, [args...]).
    """
    manager_sc = scval.from_address(manager_kp.public_key)
    blend_sc   = scval.from_address(StrKey.encode_contract(StrKey.decode_contract(blend_id)))

    fn = stellar_xdr.SorobanAuthorizedFunction(
        type=stellar_xdr.SorobanAuthorizedFunctionType.SOROBAN_AUTHORIZED_FUNCTION_TYPE_CONTRACT_FN,
        contract_fn=stellar_xdr.InvokeContractArgs(
            contract_address=stellar_xdr.ScAddress(
                type=stellar_xdr.ScAddressType.SC_ADDRESS_TYPE_CONTRACT,
                contract_id=stellar_xdr.Hash(StrKey.decode_contract(vault_id)),
            ),
            function_name=stellar_xdr.ScSymbol(sc_symbol=b"execute_op"),
            args=[
                manager_sc,
                blend_sc,
                scval.from_symbol(fn_name_str),
                inner_args_sc,
            ],
        ),
    )
    root_invocation = stellar_xdr.SorobanAuthorizedInvocation(
        function=fn,
        sub_invocations=[],
    )

    # Nonce: use a random 64-bit value
    import random
    nonce = random.randint(-(2**63), 2**63 - 1)

    credentials = stellar_xdr.SorobanAddressCredentials(
        address=stellar_xdr.ScAddress(
            type=stellar_xdr.ScAddressType.SC_ADDRESS_TYPE_ACCOUNT,
            account_id=stellar_xdr.AccountID(
                account_id=stellar_xdr.PublicKey(
                    type=stellar_xdr.PublicKeyType.PUBLIC_KEY_TYPE_ED25519,
                    ed25519=stellar_xdr.Uint256(manager_kp.raw_public_key()),
                )
            ),
        ),
        nonce=stellar_xdr.Int64(nonce),
        signature_expiration_ledger=stellar_xdr.Uint32(expiration_ledger),
        signature=stellar_xdr.ScVal(stellar_xdr.ScValType.SCV_VOID),
    )

    entry = stellar_xdr.SorobanAuthorizationEntry(
        credentials=stellar_xdr.SorobanCredentials(
            type=stellar_xdr.SorobanCredentialsType.SOROBAN_CREDENTIALS_ADDRESS,
            address=credentials,
        ),
        root_invocation=root_invocation,
    )

    # Sign the auth entry
    preimage = stellar_xdr.HashIDPreimage(
        type=stellar_xdr.EnvelopeType.ENVELOPE_TYPE_SOROBAN_AUTHORIZATION,
        soroban_authorization=stellar_xdr.HashIDPreimageSorobanAuthorization(
            network_id=stellar_xdr.Hash(
                hash=Network.PUBLIC_NETWORK_PASSPHRASE.encode()[:32]
                # Actually the network_id is SHA256 of the passphrase
            ),
            nonce=stellar_xdr.Int64(nonce),
            signature_expiration_ledger=stellar_xdr.Uint32(expiration_ledger),
            invocation=root_invocation,
        ),
    )
    import hashlib
    network_id_bytes = hashlib.sha256(Network.PUBLIC_NETWORK_PASSPHRASE.encode()).digest()
    preimage2 = stellar_xdr.HashIDPreimage(
        type=stellar_xdr.EnvelopeType.ENVELOPE_TYPE_SOROBAN_AUTHORIZATION,
        soroban_authorization=stellar_xdr.HashIDPreimageSorobanAuthorization(
            network_id=stellar_xdr.Hash(hash=network_id_bytes),
            nonce=stellar_xdr.Int64(nonce),
            signature_expiration_ledger=stellar_xdr.Uint32(expiration_ledger),
            invocation=root_invocation,
        ),
    )
    preimage_bytes = preimage2.to_xdr_bytes()
    msg = hashlib.sha256(preimage_bytes).digest()
    signature_bytes = manager_kp.sign(msg)

    # Build the signature as ScVal map: { "public_key": bytes, "signature": bytes }
    sig_map = stellar_xdr.ScVal(
        type=stellar_xdr.ScValType.SCV_MAP,
        map=stellar_xdr.ScMap(
            sc_map=[
                stellar_xdr.ScMapEntry(
                    key=stellar_xdr.ScVal(
                        type=stellar_xdr.ScValType.SCV_SYMBOL,
                        sym=stellar_xdr.ScSymbol(sc_symbol=b"public_key"),
                    ),
                    val=stellar_xdr.ScVal(
                        type=stellar_xdr.ScValType.SCV_BYTES,
                        bytes=stellar_xdr.ScBytes(sc_bytes=manager_kp.raw_public_key()),
                    ),
                ),
                stellar_xdr.ScMapEntry(
                    key=stellar_xdr.ScVal(
                        type=stellar_xdr.ScValType.SCV_SYMBOL,
                        sym=stellar_xdr.ScSymbol(sc_symbol=b"signature"),
                    ),
                    val=stellar_xdr.ScVal(
                        type=stellar_xdr.ScValType.SCV_BYTES,
                        bytes=stellar_xdr.ScBytes(sc_bytes=signature_bytes),
                    ),
                ),
            ]
        ),
    )
    sig_vec = stellar_xdr.ScVal(
        type=stellar_xdr.ScValType.SCV_VEC,
        vec=stellar_xdr.ScVec(sc_vec=[sig_map]),
    )

    entry.credentials.address.signature = sig_vec
    return entry


# ── Main ──────────────────────────────────────────────────────────────────────

def main():
    if not MANAGER_SECRET:
        print("Error: set MANAGER_SECRET env variable")
        sys.exit(1)
    manager_kp = Keypair.from_secret(MANAGER_SECRET)
    server = SorobanServer(RPC_URL)

    print(f"Manager: {manager_kp.public_key}")
    print(f"Network: mainnet\n")

    results = {}

    for name, cfg in VAULTS.items():
        vault_id  = cfg["vault"]
        blend_id  = cfg["blend_new"]
        amount    = cfg["supply_amount"]

        print(f"{'='*60}")
        print(f"Vault: {name.upper()}  ({vault_id})")
        print(f"Blend: {blend_id}")
        print(f"Amount: {amount} units ({amount/1e7:.7f} USDC)\n")

        # ── Step 1: Try simulate to discover partial footprint ────────────────
        print("  [1] Attempting simulation of execute_op → blend.supply ...")
        account = server.load_account(manager_kp.public_key)

        host_fn = build_execute_op_invoke(
            vault_id, blend_id, manager_kp.public_key,
            BLEND_POOL, USDC_ID, amount
        )
        op = stellar_sdk.InvokeHostFunction(host_function=host_fn, auth=[])
        tx = (
            TransactionBuilder(
                source_account=account,
                network_passphrase=NETWORK_PASSPHRASE,
                base_fee=BASE_FEE,
            )
            .append_operation(op)
            .set_timeout(30)
            .build()
        )

        sim_result = server.simulate_transaction(tx.to_xdr())
        sim_error = getattr(sim_result, 'error', None)

        if sim_error:
            print(f"  Simulation error (expected): {sim_error}")
        else:
            print(f"  Simulation OK (unexpected!)")

        # Extract partial footprint from simulation
        partial_footprint = []
        rw_keys = []
        if hasattr(sim_result, 'transaction_data') and sim_result.transaction_data:
            td = stellar_xdr.SorobanTransactionData.from_xdr(sim_result.transaction_data)
            for k in td.resources.footprint.read_only:
                partial_footprint.append(k.to_xdr())
            for k in td.resources.footprint.read_write:
                partial_footprint.append(k.to_xdr())
                rw_keys.append(k.to_xdr())
            print(f"  Partial footprint: {len(partial_footprint)} keys ({len(rw_keys)} RW)")

        # ── Step 2: Add missing SAC balance keys ─────────────────────────────
        print("  [2] Augmenting footprint with SAC balance keys ...")

        extra_keys = []
        extra_rw = []

        # USDC balance for strategy (read-write: decreases when supplying)
        blend_balance_key = build_sac_balance_key(USDC_ID, blend_id)
        extra_keys.append(blend_balance_key.to_xdr())
        extra_rw.append(blend_balance_key.to_xdr())

        # USDC balance for pool (read-write: increases when supplying)
        pool_balance_key = build_sac_balance_key(USDC_ID, BLEND_POOL)
        extra_keys.append(pool_balance_key.to_xdr())
        extra_rw.append(pool_balance_key.to_xdr())

        # USDC balance for vault (read-write: vault pre-funds strategy)
        vault_balance_key = build_sac_balance_key(USDC_ID, vault_id)
        extra_keys.append(vault_balance_key.to_xdr())
        extra_rw.append(vault_balance_key.to_xdr())

        all_keys = list(set(partial_footprint + extra_keys))
        all_rw   = list(set(rw_keys + extra_rw))

        # Add instance keys for blend strategy and blend pool (if not already present)
        blend_instance = build_contract_instance_key(blend_id).to_xdr()
        pool_instance  = build_contract_instance_key(BLEND_POOL).to_xdr()
        for k in [blend_instance, pool_instance]:
            if k not in all_keys:
                all_keys.append(k)
                all_rw.append(k)

        print(f"  Total footprint: {len(all_keys)} keys ({len(all_rw)} RW)")

        # ── Step 3: Build final transaction with pre-set resources ────────────
        print("  [3] Building transaction with padded resources ...")

        # Get current ledger for expiration
        latest = server.get_latest_ledger()
        current_ledger = latest.sequence
        expiry_ledger = current_ledger + 200

        # Build auth entry for manager signing execute_op
        pool_sc   = scval.from_address(StrKey.encode_contract(StrKey.decode_contract(BLEND_POOL)))
        usdc_sc   = scval.from_address(StrKey.encode_contract(StrKey.decode_contract(USDC_ID)))
        amount_sc = scval.from_int128(amount)
        inner_args = scval.from_vec([pool_sc, usdc_sc, amount_sc])

        auth_entry = get_manager_auth_entry(
            server, manager_kp,
            vault_id, blend_id,
            "supply", inner_args,
            expiry_ledger,
        )

        # Reload account to get fresh sequence number
        account = server.load_account(manager_kp.public_key)
        host_fn2 = build_execute_op_invoke(
            vault_id, blend_id, manager_kp.public_key,
            BLEND_POOL, USDC_ID, amount
        )
        op2 = stellar_sdk.InvokeHostFunction(
            host_function=host_fn2,
            auth=[auth_entry],
        )

        soroban_data = build_padded_soroban_data(all_keys, all_rw)

        tx2 = (
            TransactionBuilder(
                source_account=account,
                network_passphrase=NETWORK_PASSPHRASE,
                base_fee=BASE_FEE,
            )
            .append_operation(op2)
            .set_timeout(60)
            .build()
        )
        tx2.transaction.soroban_data = soroban_data

        # Sign the envelope with manager's key
        tx2.sign(manager_kp)

        print(f"  TX XDR (supply): {tx2.to_xdr()[:80]}...")

        # ── Step 4: Submit supply ─────────────────────────────────────────────
        print("  [4] Submitting supply transaction ...")
        try:
            resp = server.send_transaction(tx2.to_xdr())
            supply_hash = resp.hash
            print(f"  Supply submitted: {supply_hash}")

            # Wait for confirmation
            for _ in range(30):
                time.sleep(5)
                status = server.get_transaction(supply_hash)
                if status.status != "NOT_FOUND":
                    break

            if status.status == "SUCCESS":
                print(f"  Supply SUCCESS: {supply_hash}")
                results[name] = {"supply": supply_hash, "status": "ok"}
            else:
                print(f"  Supply FAILED: status={status.status}")
                if hasattr(status, 'result_xdr') and status.result_xdr:
                    print(f"  Result: {status.result_xdr}")
                results[name] = {"supply": supply_hash, "status": "failed"}
                continue  # Skip withdraw if supply failed

        except Exception as e:
            print(f"  Submit error: {e}")
            results[name] = {"status": "submit_error", "error": str(e)}
            continue

        # ── Step 5: Withdraw (same amount) ────────────────────────────────────
        print(f"\n  [5] Building withdraw_from_lending transaction ...")
        time.sleep(5)

        account = server.load_account(manager_kp.public_key)
        inner_args_w = scval.from_vec([pool_sc, usdc_sc, amount_sc])

        auth_entry_w = get_manager_auth_entry(
            server, manager_kp,
            vault_id, blend_id,
            "withdraw_from_lending", inner_args_w,
            expiry_ledger,
        )

        host_fn_w = build_withdraw_invoke(
            vault_id, blend_id, manager_kp.public_key,
            BLEND_POOL, USDC_ID, amount
        )
        op_w = stellar_sdk.InvokeHostFunction(
            host_function=host_fn_w,
            auth=[auth_entry_w],
        )

        # Withdraw doesn't need authorize_as_current_contract so simulation should work
        tx_w_temp = (
            TransactionBuilder(
                source_account=account,
                network_passphrase=NETWORK_PASSPHRASE,
                base_fee=BASE_FEE,
            )
            .append_operation(op_w)
            .set_timeout(60)
            .build()
        )
        sim_w = server.simulate_transaction(tx_w_temp.to_xdr())
        if getattr(sim_w, 'error', None):
            print(f"  Withdraw simulation error: {sim_w.error}")
            # Use padded resources like supply
            tx_w = tx_w_temp
            tx_w.transaction.soroban_data = build_padded_soroban_data(all_keys, all_rw)
        else:
            print("  Withdraw simulation OK")
            tx_w = stellar_sdk.helpers.parse_transaction_envelope_from_xdr(
                sim_w.transaction, network_passphrase=NETWORK_PASSPHRASE
            )
            # Replace auth with our signed entry
            for op in tx_w.transaction.transaction.operations:
                if hasattr(op, 'auth'):
                    op.auth = [auth_entry_w]

        tx_w.sign(manager_kp)

        print(f"  [6] Submitting withdraw transaction ...")
        try:
            resp_w = server.send_transaction(tx_w.to_xdr())
            withdraw_hash = resp_w.hash
            print(f"  Withdraw submitted: {withdraw_hash}")

            for _ in range(30):
                time.sleep(5)
                status_w = server.get_transaction(withdraw_hash)
                if status_w.status != "NOT_FOUND":
                    break

            if status_w.status == "SUCCESS":
                print(f"  Withdraw SUCCESS: {withdraw_hash}")
                results[name]["withdraw"] = withdraw_hash
            else:
                print(f"  Withdraw FAILED: {withdraw_hash}")
                results[name]["withdraw"] = withdraw_hash
                results[name]["withdraw_status"] = "failed"

        except Exception as e:
            print(f"  Withdraw submit error: {e}")
            results[name]["withdraw_error"] = str(e)

    # ── Summary ───────────────────────────────────────────────────────────────
    print(f"\n{'='*60}")
    print("Results:")
    for vault_name, r in results.items():
        print(f"  {vault_name}: {json.dumps(r, indent=4)}")

    return results


if __name__ == "__main__":
    import stellar_sdk  # re-import for top-level use in ops
    main()
