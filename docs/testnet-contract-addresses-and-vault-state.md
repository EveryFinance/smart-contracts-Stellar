# Testnet Contract Addresses and Vault State

**Network:** Stellar Testnet  
**Deployment date:** 2026-05-13  
**Singleton env:** `deployments/testnet-protocol-environment-20260513-035644.env`  
**Vault env:** `deployments/testnet-protocol-environment-20260513-060135.env`  
**State snapshot:** 2026-05-13 at ~06:44 UTC

---

## Protocol Singletons

These contracts are deployed once and shared by all vaults.

| Contract | Address |
|----------|---------|
| AssetHandler | `CAPU2HYZLZHION6CAJGISVQ5YZUYIDNFI6GREAUJXAVRY7OIVPZYNYDU` |
| Oracle | `CCPO4KFTALNJLHC7N5IW5RQCQBNHL5C4E37U7YZGN5DQYXO54RNPH74C` |
| Factory | `CD5NO5UFMKOLCJLSU2VYHJGYCUY5M3PDAICHG36GLGKV3DHGSNJZNOKZ` |

---

## Mock Asset Contracts

All assets use **7 decimal places** (1 token = 10,000,000 raw units).  
`mint` is permissionless — anyone can call it without authorization.

| Asset | Contract Address | Oracle price (1e7) | Human price |
|-------|-----------------|-------------------|-------------|
| USDC | `CD37EBBWP3RY4QRIBP4D7KJGN6GEFXNHF5ZDMXSKSHVTLJMZ3CSIQMP7` | 10,000,000 | $1.00 |
| XLM | `CCF7A4NRMV7IBS3U2AZLVO7CIU5OOHGCJGZIMQ7NBTZXD3PQOZQP4OXZ` | 1,700,000 | $0.17 |
| BTC | `CCETRE5YXNOURFLOQ5BC2WK6WPNQ2FZ66OUS46QRJUEPFJ76SGRQTZ7U` | 816,844,300,000 | $81,684.43 |
| PYUSD | `CDJQVOGUPGJHYM77CNK55I4JCD4DA6BDLQWMSCGH6IDHILZJSZVNKSPW` | 10,000,000 | $1.00 |
| EURC | `CAEZLZANE4RCDZUC4DMQBCLETU2SB5Z44V5DSSF7TJZWZ73R25JFTK2B` | 10,800,000 | $1.08 |
| AQUA | `CDEWXZRKPFU7AIXVVU6GMYZH46G5DLFTCYF7HP2ORC2MAG6FQWD5FPYD` | 4,500 | $0.00045 |
| USTRY | `CCOPVGXNWL4LRFDYXTCM3WLWHMZCFZSBJ2TSSMQBFTX2HTMIH4HEVOIZ` | 10,300,000 | $1.03 |

---

## How to Mint USDC (Stellar Expert Explorer)

USDC minting requires no authorization. You can call it directly from the browser.

### Step 1 — Open the contract on Stellar Expert

Go to:
```
https://stellar.expert/explorer/testnet/contract/CD37EBBWP3RY4QRIBP4D7KJGN6GEFXNHF5ZDMXSKSHVTLJMZ3CSIQMP7
```

### Step 2 — Verify the contract exposes `mint`

On the contract page, click **Contract Interface** (or **Invoke**). You will see the list of exported functions. Confirm `mint(to: Address, amount: i128)` is listed. No admin check exists in the source — the function mints freely to any address.

### Step 3 — Invoke `mint` from Stellar Lab

Open Stellar Lab contract invoke for testnet:
```
https://lab.stellar.org/
```

Fill in:
- **Network:** Testnet
- **Contract ID:** `CD37EBBWP3RY4QRIBP4D7KJGN6GEFXNHF5ZDMXSKSHVTLJMZ3CSIQMP7`
- **Function:** `mint`
- **`to`:** your testnet address (G…)
- **`amount`:** e.g. `1000000000` = 100 USDC (7 decimals)

Sign and submit with any funded testnet account.

### Step 4 — Via Stellar CLI (alternative)

```bash
stellar contract invoke \
  --network testnet \
  --source-account <your-key-name> \
  --id CD37EBBWP3RY4QRIBP4D7KJGN6GEFXNHF5ZDMXSKSHVTLJMZ3CSIQMP7 \
  -- mint \
  --to <G...YOUR_ADDRESS> \
  --amount 1000000000
```

`1000000000` = 100 USDC (divide by 10,000,000 to get human amount).

---

## Beta Vault — USDC · XLM · PYUSD · EURC · AQUA · USTRY

Strategies: Blend (lending) + Soroswap LP + Phoenix LP

### Contract Addresses

| Contract | Address |
|----------|---------|
| Vault | `CBR6GCCBB73UMQOHUXWFS4L5BL7X7GUR3EPM5YF4OWU6LRNBTARPQWTR` |
| Share Token | `CBDQVVDZWGVVHELG25T2WQ4H3EN2BSWLS552D5JJUSBMRFYPTEGCOF3O` |
| Blend Pool | `CDI4PEPUQI5BNIXHXUEE4KMV3CMQAKUOCIN6A4FIV4TGDGFG75H3DJXQ` |
| Blend Strategy | `CD2BH6LSAUALSFIYQXJXTSLKGHVOM2CN7REY63FWJ3456CRV6A5RKX6K` |
| Soroswap Router | `CBTSWD4SZMB3G62XDTXMQHZ3U2ESQU66VJGOXV6ZBIVVS3SP6PJYLJ7B` |
| Soroswap Strategy | `CAJOBH3D3N5OPMV6M3C262ZPTBTHHND6GTTPNMU2Y3RHCIDNTVA776YV` |
| Phoenix Pool | `CANSI3MFRCE73JBGA54YHLELH7J52ALOC3MNSBNQOAFEDWHKHHW23HMC` |
| Phoenix Strategy | `CAUWEKQOAYPMLONHMYQGQ2LIVFBSUOD47WWFFP5J7CI62WHVK6R3FIRL` |

### Live State (snapshot 2026-05-13 ~06:44 UTC)

| Metric | Raw (1e7) | Human |
|--------|-----------|-------|
| NAV | 4,650,000,841 | **465.00 USDC** |
| Share token price | 9,999,995 | **≈ 1.00 USDC/share** |
| Total share supply | 4,650,003,097 | 465.00 shares |

### Portfolio Assets (idle balances)

| Asset | Raw | Human |
|-------|-----|-------|
| USDC | 4,552,363,025 | 455.24 USDC |
| XLM | 382,893,393 | 38.29 XLM |
| USTRY | 31,598,000 | 3.16 USTRY |

### Strategy Values

| Strategy | Value |
|----------|-------|
| Blend (lending) | 0 USDC (position unwound) |
| Soroswap LP | 0 USDC (position unwound) |
| Phoenix LP | 0 USDC (position unwound) |

---

## Alpha Vault — USDC · XLM · BTC

Strategies: Blend (lending) + Soroswap LP + Phoenix LP

### Contract Addresses

| Contract | Address |
|----------|---------|
| Vault | `CALILOQJXLIYF6FNAXLA7RLGBEMGKZTSRJ5YHVF77IYBW2VURZJKHGVD` |
| Share Token | `CDJVVYFMYVXACCBC2OG5QYPEGFTNLE76MV57FIG6ZZNY4FOIRVJTY7YC` |
| Blend Pool | `CCOSAAVPWYN3IJRH2MSFV24JBSIEAPTN6TDYTROSC66MFNSE7ZDACD5S` |
| Blend Strategy | `CA53EQJ5UOOCB7URSRYRO3XQ3K77UNO6X7P3EKYNNXVHE2SSBW6IH5KR` |
| Soroswap Router | `CAESUGB7MF34XYMPQNXULDFHF7C43T3DZAK4KNN7QTJMWEZR2ISAR5HR` |
| Soroswap Strategy | `CAQDKLPAHAL4MCARAF6TRJYCSXIRMYIWCP57AFYSNM7AMTGTAXPJPKPU` |
| Phoenix Pool | `CDBZN3GQ6H6WP366AU2AHEEGEBJPZUCGXMUJAWOM7534QIAEB2AVUFBJ` |
| Phoenix Strategy | `CDFYUFTBBDO4VU2PUISFXNIK6NFTMK3FNPHMJEQLJZGCLITF3IB7GG3Y` |

### Live State (snapshot 2026-05-13 ~06:44 UTC)

| Metric | Raw (1e7) | Human |
|--------|-----------|-------|
| NAV | 4,650,172,902 | **465.02 USDC** |
| Share token price | 10,000,661 | **≈ 1.00 USDC/share** |
| Total share supply | 4,649,865,442 | 464.99 shares |

### Portfolio Assets (idle balances)

| Asset | Raw | Human |
|-------|-----|-------|
| USDC | 4,552,326,059 | 455.23 USDC |
| XLM | 382,890,517 | 38.29 XLM |
| BTC | 401 | 0.0000401 BTC (~$3.27) |

### Strategy Values

| Strategy | Value |
|----------|-------|
| Blend (lending) | 0 USDC (position unwound) |
| Soroswap LP | 0 USDC (position unwound) |
| Phoenix LP | 0 USDC (position unwound) |

---

## Gamma Vault — USDC · USTRY

Strategies: Blend (lending) only

### Contract Addresses

| Contract | Address |
|----------|---------|
| Vault | `CDAMFVIKNVBWSSW2NPKUV5DBBQ44T4S2YCYK3OEGTNCKSHRHED5GVUNX` |
| Share Token | `CASCW3KOU46LDGCKHWPBGRCW5F5PMLMB3GUBM4QAAN62KPAN5GM2DLTV` |
| Blend Pool | `CCSWDI2UR2HFGDAQXJ5A5KU5UVP5WPXRNKV3OI3BVUF32ZLLPFWW3C46` |
| Blend Strategy | `CAFDDR7UUEQSJ656GMZK5OZLGIDX6VKCEPMNMOQZNDCUYOW3LP3MBANV` |

### Live State (snapshot 2026-05-13 ~06:44 UTC)

| Metric | Raw (1e7) | Human |
|--------|-----------|-------|
| NAV | 4,650,000,826 | **465.00 USDC** |
| Share token price | 9,999,995 | **≈ 1.00 USDC/share** |
| Total share supply | 4,650,003,033 | 465.00 shares |

### Portfolio Assets (idle balances)

| Asset | Raw | Human |
|-------|-----|-------|
| USDC | 4,650,000,826 | 465.00 USDC |

### Strategy Values

| Strategy | Value |
|----------|-------|
| Blend (lending) | 0 USDC (position unwound) |
