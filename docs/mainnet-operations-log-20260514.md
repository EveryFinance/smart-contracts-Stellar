# Mainnet Investor and Manager Transaction Examples

Records all investor (user) and manager transactions executed on the three
mainnet vaults since deployment.

---

## Investor Transactions — 2026-05-14

**Account:** Admin — `GDZN5WVOUBRTZKADUULRXJOGVGNK4WWXJPSALTVOORLSU5NFOZ2P5NCB`  
**Asset:** USDC — `CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75`

### Alpha Vault — `CAHHS2EFYMKKBYP3LTUFPDS2JSY4EPQLUDEUOFI5I7P7MOIBSJH5AUKQ`

Portfolio: USDC / XLM / BTC · Share token: `CCHY2XQNGAWYM755KIM7PKE6LEMQVIVVQJIKMCAA5EFNK5NQQVP2O7A6`

| Operation | Amount | Shares | Tx hash | Explorer |
|-----------|--------|--------|---------|----------|
| `deposit` | 1.0000000 USDC (10,000,000 units) | 10,000,000 minted | `e59210e0...` | [view tx](https://stellar.expert/explorer/public/tx/e59210e029959282a71f9b03552cd28cb74c8f600fce9659c6c0411fd52d78d3) |
| `withdraw` (50%) | 5,000,000 shares burned | 4,999,997 USDC returned | `d43bb8a9...` | [view tx](https://stellar.expert/explorer/public/tx/d43bb8a90bf69974b260509751d3e0da7300473e530b2e869fa6bb5b0b9d62a2) |

### Beta Vault — `CDYB5FK54OXV36AQ2TBK6V2K6KYN6RXNIID6HMCYUP7EJ4BEV7BAVIYB`

Portfolio: USDC / XLM / PYUSD / EURC / AQUA · Share token: `CDCM5W7XOPAZC3FOUKKWUEY7BGLMDXPYJHBFUNBXJXYJCZNMY4JNE55E`

| Operation | Amount | Shares | Tx hash | Explorer |
|-----------|--------|--------|---------|----------|
| `deposit` | 1.0000000 USDC (10,000,000 units) | 10,000,000 minted | `42278359...` | [view tx](https://stellar.expert/explorer/public/tx/4227835904d33630937c970b20e95f0bfddded9fff11214927f4afacd47cd0ae) |
| `withdraw` (50%) | 5,000,000 shares burned | 4,999,997 USDC returned | `a1533c07...` | [view tx](https://stellar.expert/explorer/public/tx/a1533c07b237da99b1417a1c5568cc76c70fb93d4768c900b5a11ea7186c2380) |

### Gamma Vault — `CCHJFS4OEKTLJLLL6OQXFIS2ECTJRA6WMUNXVS7MD6DKIIB6RTXNBZRW`

Portfolio: USDC only · Share token: `CDY5C7DAWSXF536ORWAEXKHWSDEWOMWWQTXV32IGGJMRYDXTDLE33DSM`

| Operation | Amount | Shares | Tx hash | Explorer |
|-----------|--------|--------|---------|----------|
| `deposit` | 1.0000000 USDC (10,000,000 units) | 10,000,000 minted | `133ce20b...` | [view tx](https://stellar.expert/explorer/public/tx/133ce20b11ef3ab0a1dd02b54146cf23f572f7d87526f4fc4f386395e790c761) |
| `withdraw` (50%) | 5,000,000 shares burned | 4,999,997 USDC returned | `2bc9d790...` | [view tx](https://stellar.expert/explorer/public/tx/2bc9d790a5151cee22af188f042f4cd0edf0640cac4e0791f8792f8de25b0204) |

---

## Manager Transactions — 2026-05-18 (Blend lending)

**Manager:** `GCDXIPE5MSBFCYXNM2MP322PGWJWE45T7XJQR73TCY7CC5XIENTSFELX`  
**Blend V2 pool:** `CAJJZSGMMM3PD7N33TAPHGBUGTB43OC73HVIK2L2G6BNGGGYOSSYBXBD`  
**Asset:** USDC — `CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75`  
**Amount per vault:** 0.1 USDC (1,000,000 units)

Blend strategy addresses in use:

| Vault | Blend strategy |
|-------|---------------|
| Alpha | `CBD7QEXZP2RVIEFD4OUWRDAXKB2BM4GKUME3EZQQEBWAABL3IEGXPGKI` |
| Beta  | `CBO5XSLPO4DCJJSWWWCPHZ6JDFKPFBPMQDLJWUCJJ3PEWRYO7V6JOJ7V` |
| Gamma | `CDMPATIFU2P7JRRAQZZ3655IZSNON62V3EUZK2UZH33C7ACQF6EQ2HYM` |

### Alpha Vault — `CAHHS2EFYMKKBYP3LTUFPDS2JSY4EPQLUDEUOFI5I7P7MOIBSJH5AUKQ`

| Operation | Amount | Tx hash | Explorer |
|-----------|--------|---------|----------|
| `execute_op → supply` | 1,000,000 units (0.1 USDC) | `c296509f...` | [view tx](https://stellar.expert/explorer/public/tx/c296509f5d320ab8428aa21495c8a4eea9895c26d3d3d77839f6eba63eab902b) |
| `execute_op → withdraw_from_lending` | 1,000,000 units | `a6831f7d...` | [view tx](https://stellar.expert/explorer/public/tx/a6831f7db5f5d3155fec3983ebd7c08054231b2cf5ac626a8fee0d3c2951a33c) |

### Beta Vault — `CDYB5FK54OXV36AQ2TBK6V2K6KYN6RXNIID6HMCYUP7EJ4BEV7BAVIYB`

| Operation | Amount | Tx hash | Explorer |
|-----------|--------|---------|----------|
| `execute_op → supply` | 1,000,000 units (0.1 USDC) | `2f2f0db2...` | [view tx](https://stellar.expert/explorer/public/tx/2f2f0db28acc5a39c7a39212daaa3f33e359f94d2365ed4d0baf4af1a1197920) |
| `execute_op → withdraw_from_lending` | 1,000,000 units | `fcc854ae...` | [view tx](https://stellar.expert/explorer/public/tx/fcc854ae57feab9b089718db6620f11b6db6846857c0720a22d273c101706cfe) |

### Gamma Vault — `CCHJFS4OEKTLJLLL6OQXFIS2ECTJRA6WMUNXVS7MD6DKIIB6RTXNBZRW`

| Operation | Amount | Tx hash | Explorer |
|-----------|--------|---------|----------|
| `execute_op → supply` | 1,000,000 units (0.1 USDC) | `0eab7bda...` | [view tx](https://stellar.expert/explorer/public/tx/0eab7bdad92ba5d88a70d48a3ed3b73215ec3975afe2df675fb49dadf725af9b) |
| `execute_op → withdraw_from_lending` | 1,000,000 units | `deb43ae2...` | [view tx](https://stellar.expert/explorer/public/tx/deb43ae2f7b4e8d27dad4738f3d72ba741427558b7790cfbd0ded08c4ba20373) |

---

## Investor Transactions — 2026-05-18

**User account (admin):** `GDZN5WVOUBRTZKADUULRXJOGVGNK4WWXJPSALTVOORLSU5NFOZ2P5NCB`  
**Asset:** USDC — `CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75`

### Alpha Vault — `CAHHS2EFYMKKBYP3LTUFPDS2JSY4EPQLUDEUOFI5I7P7MOIBSJH5AUKQ`

Share token: `CCHY2XQNGAWYM755KIM7PKE6LEMQVIVVQJIKMCAA5EFNK5NQQVP2O7A6`

| Operation | Amount | Shares | Tx hash | Explorer |
|-----------|--------|--------|---------|----------|
| `deposit` | 1.0000000 USDC (10,000,000 units) | 10,002,134 minted | `7c22f561...` | [view tx](https://stellar.expert/explorer/public/tx/7c22f561e58e37501b6c4c02ee8728dc1b20855752c7fd74c2e0e8f83c823c38) |
| `withdraw` | 10,002,134 shares burned | 9,999,999 USDC returned | `3a5d8cec...` | [view tx](https://stellar.expert/explorer/public/tx/3a5d8cec433be77d4b9d901ab6e6c71c1aceefd236a8f36ba05be4fa70224b9c) |

### Beta Vault — `CDYB5FK54OXV36AQ2TBK6V2K6KYN6RXNIID6HMCYUP7EJ4BEV7BAVIYB`

Share token: `CDCM5W7XOPAZC3FOUKKWUEY7BGLMDXPYJHBFUNBXJXYJCZNMY4JNE55E`

| Operation | Amount | Shares | Tx hash | Explorer |
|-----------|--------|--------|---------|----------|
| `deposit` | 1.0000000 USDC (10,000,000 units) | 10,002,133 minted | `effe6dd1...` | [view tx](https://stellar.expert/explorer/public/tx/effe6dd15e129641ca2f4cb7b3df6e033f341a1d0c84e9ec0b2c8dec837554e1) |
| `withdraw` | 10,002,133 shares burned | 9,999,998 USDC returned | `c1a4f6de...` | [view tx](https://stellar.expert/explorer/public/tx/c1a4f6def29298abed96c3ae187acb898e4fa1ab597f69aaea07b326cf52f7a2) |

### Gamma Vault — `CCHJFS4OEKTLJLLL6OQXFIS2ECTJRA6WMUNXVS7MD6DKIIB6RTXNBZRW`

Share token: `CDY5C7DAWSXF536ORWAEXKHWSDEWOMWWQTXV32IGGJMRYDXTDLE33DSM`

| Operation | Amount | Shares | Tx hash | Explorer |
|-----------|--------|--------|---------|----------|
| `deposit` | 0.4000000 USDC (4,000,000 units) | 4,000,852 minted | `475239f0...` | [view tx](https://stellar.expert/explorer/public/tx/475239f0e37f3e592f7f6069f256f3d8ee23e37718cf7c091e1328d51636505a) |
| `withdraw` | 4,000,852 shares burned | 3,999,999 USDC returned | `122487fd...` | [view tx](https://stellar.expert/explorer/public/tx/122487fd72e61ff2afc2e51e98d434daee1375dba2c655bfa964027a2070ee8e) |

---

## Observations

- **Management fee behavior:** Each deposit and withdrawal triggers fee accrual. Small mgmt fee shares are minted to the treasury on each operation — consistent with the 2% annual fee pro-rated over the time held.
- **Cooldown enforced:** The 60-second cooldown was respected between deposits and withdrawals.
- **Share price > 1.0 on 2026-05-18:** More shares were minted per USDC deposited than on 2026-05-14 (e.g. Alpha: 10,002,134 shares for 10,000,000 USDC) reflecting accumulated yield from prior activity.
- **Exit fee:** The 1–2 stroop difference between USDC deposited and returned is the exit fee remaining in the vault as protocol revenue.
