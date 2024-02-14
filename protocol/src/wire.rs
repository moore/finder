
use crate::{
    crypto::ChannelId, sync::{
        SyncRequest,
        SyncResponse,
    }, NodeId
};


use serde::{Deserialize, Serialize};

mod packets;

pub use self::packets::{WireReader, WireWriter};

#[derive(Debug)]
pub enum WireError {
    Unreachable,
    OutOfBounds,
    DeserializeError(postcard::Error),
    WrongBlock(u16),
    NotPacket,
}

impl From<postcard::Error> for WireError {
    fn from(value: postcard::Error) -> Self {
        WireError::DeserializeError(value)
    }
}


#[derive(Debug, Serialize, Deserialize)]
pub struct ChannelInfo {
    pub channel_id: ChannelId,
    //channel_state: ChanelState,
    pub message_count: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum NetworkProtocol<const MAX_CHANNELS: usize, const MAX_NODES: usize, const RESPONSE_MAX: usize> {
    Hello {
        pub_key_id: NodeId,
        peer_count: u8,
        channel_info: heapless::Vec<ChannelInfo, MAX_CHANNELS>,
    },
    SyncRequest(SyncRequest<MAX_NODES>),
    SyncResponse(SyncResponse<RESPONSE_MAX>),
}


/// - Send hello every n seconds.
///   + each hello should send different channel info 
///     until they have all be sent and the start over.
///     this way we can keep hello's in a single packet.
///     but eventually announce all the channels we know.
///     Hello should specify if sender can add users to channel.
///   + Delay hello if non hello traffic is observed.
/// 
/// - fn list_known_channels()
/// 
/// - request add message which is p2p
/// 
/// - If hello indicates receiving client is missing content
///   send SyncRequest content with random delay. If SyncRequest
///   is observed from other device try to piggy back off it and
///   try to avoid sending while channel is busy.
/// 
/// - If SyncResponse is seen of channel try to consume even it 
///   it was not requested. 
/// 
/// - If new message is generated locally send SyncResponse with new message(s)
/// 
/// - There should be three rings of packets: 
/// 
///   + Important: Always try to send and send with high redundancy. 
///     ( Used for alerts )
/// 
///   + Normal: Send where there is no ring 0 packets to send with redundancy
///     based on expected loss due to RSSI.
///     (Used for normal messages)
/// 
///   + Hello: Send with high redundancy but back off (with random delay) if traffic is observed
///     from other senders.
///     (This should be used for hellos only)
/// 
///  Note: all messages except request add are sent broadcast.

#[cfg(test)]
mod test;