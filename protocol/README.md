# nexuscore


## Sync Strategy

We want to sync over packet networks like LoRa and ESP-Now which have small packet sizes. In addition LoRa, and broadcast ESP-Now both will likely have high packet loss in the environments we want to use them in.

For framing and error correction the idea is to use raptorq (https://github.com/cberner/raptorq).

### Discovery

Esp-Now uses a 250 byte MTU and LoRa uses a 256 byte MUT as such we will limit our discovery packets to 250 bytes. Given this we will transmit hello messages containing the hash of the public key of the device rather then the key itself. This will then use 32 bytes for a sha256.

Packet
[1b block num][1b repair count][2b len][MTU - 4]


```rust

struct ChannelInfo {
    channel_id: ChannelId,
    channel_state: ChanelState,
    message_count: u64,
}

enum NetworkProtocol {
    Hello {
        pub_key_id: [u8: 32],
        peer_count: u8,
        channel_info: Vec<ChannelInfo>,
    }                         
    SyncRequest(SyncRequest),
    SyncResponse(SyncResponse),
}
```