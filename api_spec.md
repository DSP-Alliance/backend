# Backend API Spec

## Post Requests

### /filecoin/vote?fip_number=1

Query parameter `fip_number` is used to specify which FIP is being voted on. The accompanying json body is built like the following

```json
{
    "signature": "0x67ae6539cd110b9a043e3836303771d8a8ec13c7c688f369cc1a8a9f997128bf207319c7e94a60f9739c51510cb483c8f0c2efa32147690ae8221c08d34352ec1b",
    "message": "YAY: FIP-1"
}
```

The signature is 65 bytes produced from signing the `"message"` field

The message starts with either `YAY`, `NAY`, or `ABSTAIN` followed by a colon and a space. Then `FIP-` and the number of the FIP being voted on.

For example: `YAY: FIP-123`, `NAY: FIP-1`, or `ABSTAIN: FIP-789`

This is the main endpoint being hit from the frontend to cast votes.

If the vote is in progress then a 403 error will be returned. If the vote does not exist then a 404 error will be returned.

## GET Requests

### /filecoin/vote?fip_number=1&network=mainnet

Query parameter `fip_number` is used to specify which FIP to pull votes for. The parameter `network` specifies which network to poll votes from. Some addresses are only registered to vote on testnet as they are only miners on testnet. `network` can be either `mainnet` or `calibration`.

If the vote is in progress then a 403 error will be returned. If the vote does not exist then a 404 error will be returned. If the vote has concluded then the results will be returned in json as follows

```json
    {
        "yay": 123,
        "nay": 123,
        "abstain": 123,
        "yay_storage_size": 2048,
        "nay_storage_size": 2048,
        "abstain_storage_size": 2048
    }
```

The storage size is in bytes.

### /filecoin/delegates?network=mainnet&address=0x0000000000000000000000000000000000000000

Query parameter `network` specifies which network to poll votes from. Some addresses are only registered to vote on testnet as they are only miners on testnet. `network` can be either `mainnet` or `calibration`. The `address` parameter is the 20 byte hex address which miners have delegated their votes to.

The returned json will be in the format below.

```json
    [
        "f0123",
        "f0124",
        "f0125"
    ]
```

### /filecoin/votingpower?network=mainnet&address=0x0000000000000000000000000000000000000000

Query parameter `network` specifies which network to poll votes from. Some addresses are only registered to vote on testnet as they are only miners on testnet. `network` can be either `mainnet` or `calibration`. The `address` parameter is the 20 byte hex address which miners have delegated their votes to.

The HTTP body returned will be a unsigned 128 bit integer for the voting power in bytes.
