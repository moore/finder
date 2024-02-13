
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


#[cfg(test)]
mod test;