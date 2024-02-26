
use core::{hash::Hash, marker::PhantomData, fmt::Debug};

use log;

use crate::{
    crypto::ChannelId, 
    crypto::Crypto,
    sync::{
        SyncRequest,
        SyncResponse,
    },
    storage::IO,
    NodeId,
    Client,
};


use serde::{Deserialize, Serialize};
use heapless::{
    Vec,
    FnvIndexMap,
};

use postcard::{from_bytes, to_slice};

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

struct Receiver{
    last_completed: Option<u16>,
    reader: Option<WireReader>,
}
pub struct WireState<
const MAX_CHANNELS: usize, 
const MAX_NODES: usize,
const MAX_RESPONSE: usize,
I: IO,
P: Crypto,
A,
> {
    last_received: u64,  // Epoch ms
    next_hello: u64,     // Epoch ms
    hello_duration: u64, // Duration ms
    next_message_number: u16,
    receivers: FnvIndexMap<A, Receiver, MAX_NODES>,
    mtu: u16,
    _io: PhantomData<I>,
    _crypto: PhantomData<P>,
}

pub struct PollResult {
    pub next_poll: u64,
    pub writer: Option<WireWriter>,
}

impl<
const MAX_CHANNELS: usize, 
const MAX_NODES: usize,
const MAX_RESPONSE: usize,
I: IO,
P: Crypto,
A: Eq + PartialEq + Hash + Debug + Clone,
> WireState<MAX_CHANNELS, MAX_NODES, MAX_RESPONSE, I, P, A> {
    pub fn new(mtu: u16) -> Self {
        Self {
            last_received: 0,
            next_hello: 0,
            hello_duration: 5000,
            next_message_number: 0,
            receivers: FnvIndexMap::new(),
            mtu,
            _io: PhantomData,
            _crypto: PhantomData,
        }
    }

    pub fn receive_packet(&mut self, data: &[u8], from: A) -> Result<(), WireError> {

        let received_message_number = match WireReader::check_packet(data) {
            Ok(number) => number,
            Err(e) => {
                log::info!("check packet failed {:?}", e);
                return Ok(());
            },
        };

        let receive_info = match self.receivers.get_mut(&from) {
            Some(r) => r,
            None => {
                let r = Receiver {
                    last_completed: None,
                    reader: None,
                };

                let Ok(_) = self.receivers.insert(from.clone(), r) else {
                    log::error!("to many clients! could not add {:?}", from);
                    return Ok(());
                };

                self.receivers.get_mut(&from).expect("unreachable!")
            },
        
        };

        if let Some(finished) = receive_info.last_completed {
            if received_message_number == finished {
                log::info!("received extra packet for {}", finished);
                return Ok(());
            }
        }
        if receive_info.reader.is_none() {
            let Ok(r) = WireReader::new(data, self.mtu) else {
                log::info!("Could not construct wire reader");
                return Ok(());
            };
            receive_info.reader = Some(r);
        }

        let Some(ref mut receiver) = receive_info.reader else {
            unreachable!("maybe_receiver empty after being set!!!")
        };
       
        log::info!("receiver block {}, message len {} data len {}", receiver.message_number, receiver.transfer_length, data.len());

        let result = match receiver.accept_packet(&data) {
            Ok(r) => r,
            Err(WireError::WrongBlock(_found)) => {
                let Ok(mut receiver) = WireReader::new(&data, self.mtu) else {
                    log::info!("Could not construct wire reader");
                    return Ok(());
                };
                let result = match receiver.accept_packet(&data) {
                    Ok(r) => r,
                    Err(e) => {
                        log::info!("could not accept packet because {:?}", e);
                        return Ok(());
                    }
                };
                receive_info.reader = Some(receiver);
                result
            },
            Err(e) => {
                log::info!("could not accept packet because {:?}", e);
                return Ok(());
            }
        };

        if let Some(value) = result {
            let command: NetworkProtocol<MAX_CHANNELS, MAX_NODES, MAX_RESPONSE> = from_bytes(&value)
                .expect("could not parse message");
            receive_info.last_completed = Some(received_message_number);
            log::info!("Got a result {:?}", command);
        }

        Ok(())
    }

    pub fn poll(&mut self, buffer: &mut [u8], now: u64, peer_count: u8, channel_ids: &[ChannelId], repair_count: u32, client: &Client<MAX_CHANNELS, MAX_NODES, I, P> ) -> Result<PollResult, WireError> {
        let result = if self.next_hello > now {
            PollResult {
                next_poll: self.next_hello - now,
                writer: None,
            }
            
        } else {
            self.next_hello = now + self.hello_duration;
            let hello: NetworkProtocol<MAX_CHANNELS, MAX_NODES, MAX_RESPONSE> = self.make_hello(peer_count, channel_ids, client);

            // write to buffer
            let wrote = to_slice(&hello, buffer)?;

            // make writer
            let writer = WireWriter::new(self.next_message_number, self.mtu, &wrote, repair_count);

            self.next_message_number = self.next_message_number.wrapping_add(1);

            PollResult {
                next_poll: self.hello_duration + now,
                writer: Some(writer),
            }
        };
        Ok(result)
    }

    fn make_hello(&self, peer_count: u8, channel_ids: &[ChannelId], client: &Client<MAX_CHANNELS, MAX_NODES, I, P>) -> NetworkProtocol<MAX_CHANNELS, MAX_NODES, MAX_RESPONSE> {
    
    let mut channel_info = Vec::new();

    for channel_id in channel_ids {
        let message_count = client.message_count(&channel_id)
            .expect("could not get message count");

        let info = ChannelInfo {
            channel_id: channel_id.clone(),
            message_count,
        };
        channel_info.push(info).expect("too many channels");
    }

    let node_id = client.get_node_id();
    
     NetworkProtocol::Hello {
        pub_key_id: node_id,
        peer_count,
        channel_info,
     }
}
}




#[cfg(test)]
mod test;