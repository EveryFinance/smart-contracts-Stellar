# Vaults Alpha Beta Gamma

Project: Every Finance (rebranded to Elyx Finance)

Dapp: https://www.elyx.finance/

This document records a complete on-chain showcase deployment on **Stellar testnet**.

- Deployment timestamp: `2026-04-16 19:20:20`
- Deployment env file: `deployments/showcase-three-vaults-20260416-192020.env`
- Manager: `GB2HC2NLXR7LHKXGS2IZL4F5LZVQVKRBKCWONQQW4WIYUXDILHORWQPZ`
- Demo user: `GCZZW2O23FN6IULHJF7R3JLZVQ2MCG2TYSQFYPG7WQGWUZFTT7X75RTI`

## Deployed Asset Set

- USDC (`USD Coin`): `CCIC3B2ATUPEBMQRJYOO624II6S4AKPWORKD4Z47LHXLLR5NC333H5XS`
- WETH (`Wrapped Ether`): `CA5GAWXTV5SGPW3FPS2HAUHS4JXU2VPYRFMLYMCXWCA4VPPSNX4OG7ZR`
- WBTC (`Wrapped Bitcoin`): `CBKVTLR2UG5OVU5AYYEYHJ4NJ7QMTTP2UHCD6ZT5IOXC7FP22GFBDD7B`
- XAU (`PAX Gold`): `CDH5H5Q4HFSRS3MNQQENK2TFME3VR5VZKRJVU7O7OE4ZUDMJCBUKSSPT`
- EURC (`Euro Coin`): `CDGSNI4AA5K2TU6W3HUSJQE4O4ZJ4U4OV4XCB6J2UIJO5UNKEIPL7SZ6`

## Vault Alpha (High Risk)

Objective: aggressive crypto portfolio universe.

- Asset universe (configured): `USDC`, `WETH`, `WBTC`
- Vault: `CCQALBEUASFSI5EWUYVC2E2FVH7FL4GGJK6QMJRY7EGDDY2FDVTM5SQJ`
- Share token: `CABQ3RF7ATCOKAD6S5ROKXPOIUXTS65HJNZJEAA25OTPOUMQHEKWIWYL`
- Guard: `CB5BXH3HVKHQJNOGYPVPOBTEI2TBDVZ7HSSGSFAZR7J4CJ3EQLE5RPXE`

User flow transactions:

- Deposit: https://stellar.expert/explorer/testnet/tx/8a4e97a1e8b4be88091b2515453297edd5fa7c0bbbdf1f4085b465b200ddc47b
- Withdraw: https://stellar.expert/explorer/testnet/tx/c1c8eddcbd1dc59616ad6718c9774bac2bed9457a87ba873dbc442ccb5a0e08f

## Vault Beta (Market Risk)

Objective: diversified macro market universe.

- Asset universe (configured): `USDC`, `WETH`, `WBTC`, `XAU`, `EURC`
- Vault: `CBYAJNS3UDIDKRXQFTN2HXURCC3L3KH462T4PH5IKUJHHQRCPHQ7CBCC`
- Share token: `CCC3UT3IFEUW5UDKK3NAQ6KF6YF6PQ5O45QCCA2SUIZKGQNCQ6S4RVV4`
- Guard: `CDV2GLKX3OUPWRUV2KSEHRD2HNPDGPBXUMT6S7N5C6YZGD2HT5P7X6BM`

User flow transactions:

- Deposit: https://stellar.expert/explorer/testnet/tx/54d28b6da2baf85f5ed9a5876c759c35c6d5929842ba9805ebaeb3d3c9c8aa4b
- Withdraw: https://stellar.expert/explorer/testnet/tx/9155501471a016b7ee18c5591b0586eacaccd59efaf62f30213e6d29222222ac

## Vault Gamma (Low Risk, USDC)

Objective: conservative single-asset vault (`USDC`).

- Vault: `CAYM2DPPNWTHLTC7EKD3UTNVPZUOYGOIZUCH2O3CNL6BAIH53H5JBG5Z`
- Share token: `CC6G2E7H25THCLR6GAEGGOC3PP22LEQMNHGYPI7CVSDZHMQ4ZWO3EK4A`
- Guard: `CDGAO6IMPE25WFWPTEGQQG63TO2MAGPXJZV2K5YMNUXBB2KPSYUM3DJN`
- Lending protocol adapter contracts:
  - Mock lending pool: `CCWMDBOVH543PGAEMCXQD4JB55L5JABV4PS6AHH72HT6BCBTSRI4AA2F`
  - Blend strategy: `CD6AE2TVUA4MZHF7TMG2GCOGYENJZQOSPSFQMGXC2M2OF3XZFSC5JRU2`
  - Blend strategy (auth-fix redeploy): `CC272UCB36RHYCOMLNAPJZVFYSJNLYUEYYRZWO5I627PLMMOTJ6WRAQJ`

User flow transactions:

- Deposit: https://stellar.expert/explorer/testnet/tx/ea226b7590d406a7e47c22905091c235d2b958070ed70761994d66b95ddf3165
- Withdraw: https://stellar.expert/explorer/testnet/tx/0a013ff7c13581555fd00f677138248029d46497399510d84484998c1db100d0

Gamma full lending lifecycle validation (fresh run on `2026-04-16`):

- Demo user: `GAYTDEZXAHEFVJYECMD3KZ2G3RXMX6ARGTEBE4EBXFDMT6LEKYNQWDQ5`
- Active strategy used: `CCUNZPO7ETFQP37I6WNBVRZDSPO2RU23G6GXBIOY6JZCU6ZNGXM7DCUM`
- Active lending pool used: `CDLQ3RLAMINPJTVE7NHSXEFLFFF5PRVJF6KLIZ6Z7GRNAQ5HMII2442Q`
- Deposit: https://stellar.expert/explorer/testnet/tx/5351388809691a14ee6aef3f1916bffc3eed6adc48949c4f17d56f2da4be2d37
- Invest: https://stellar.expert/explorer/testnet/tx/82da7caebb2fd92a6ac21d184e79e3b540c67e775ee00b17d29f113819bd7f68
- Yield injection: https://stellar.expert/explorer/testnet/tx/40da16e3058556af55acbdcfb6c1d4469edace38450d93b247f7864f11211940
- Withdraw: https://stellar.expert/explorer/testnet/tx/46826d6cc979ddfc111af6f4458643d40aa361a8be5b521894004d8550bdfeea

## Explorer Shortcuts (Contracts)

- Alpha Vault: https://stellar.expert/explorer/testnet/contract/CCQALBEUASFSI5EWUYVC2E2FVH7FL4GGJK6QMJRY7EGDDY2FDVTM5SQJ
- Beta Vault: https://stellar.expert/explorer/testnet/contract/CBYAJNS3UDIDKRXQFTN2HXURCC3L3KH462T4PH5IKUJHHQRCPHQ7CBCC
- Gamma Vault: https://stellar.expert/explorer/testnet/contract/CAYM2DPPNWTHLTC7EKD3UTNVPZUOYGOIZUCH2O3CNL6BAIH53H5JBG5Z
