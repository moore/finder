#![no_std]

use core::{marker::PhantomData, ops::Deref};
use heapless::{Vec, FnvIndexMap};

use postcard::{from_bytes, to_slice};
use serde::{Deserialize, Serialize};

mod channel;
use channel::*;

mod storage;
use storage::*;

pub trait Crypto {
    fn envelope_id<T>(&self, sealed: &SealedEnvelope<T>) -> EnvelopeId;
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeId(u128);

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EnvelopeId(u128);

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChannelId(u128);

#[derive(Debug, Serialize, Deserialize, Copy, Clone, Eq, PartialEq)]
pub enum Recipient {
    Node(NodeId),
    Channel(ChannelId),
}

/// Each new message records the sender, recipient
/// relative order.
///
/// The sender is specified in the `from` field and
/// verified by checking the signature.
///
/// The recipient is specified in the `to` field as
/// either a specific device (node), or a channel.
///
/// The order is defined over the `cause`, `sequence`,
/// and `sender_last` fields. It is required that the
/// `sequence` be no larger then one more then the
/// largest sequence of the last envelope received
/// from the sender in the `cause` field and
/// must also be strictly greater than the `last_sender`
/// field. The `last_sender` field must contain the `sequence`
/// value of the last `Envelope` produced by the sending node.
///
/// The reason that we use the cause sequence and
/// associated constraints is to prevent a sender
/// setting a very large sequence and exhausting the sequence counter.
///
/// In the case that the sender knows of no existing messages sent
/// to a recipient the cause field should `EnvelopeId(0)` which is
/// virtual and has a implicit `sequence of 0;
///
/// When selecting a `cause` the sending node should choose the `Envelope`
/// with the largest `sequence` value or the envelope in the case that there
/// is a tie between two or more `Envelope`s for largest `sequence`.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Envelope<T> {
    from: NodeId,
    to: Recipient,
    cause: NodeId,
    sender_last: u64,
    sequence: u64,
    data: T,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct SealedEnvelope<'a, T> {
    serialized: &'a [u8],
    signature: &'a [u8],
    _phantom: PhantomData<T>,
}

impl<'a, T> SealedEnvelope<'a, T> {
    pub fn id(&self, crypto: &impl Crypto) -> EnvelopeId {
        crypto.envelope_id(self)
    }
}


pub struct Client<const MAX_CHANNELS: usize, const MAX_NODES: usize, const MAX_RECORDS: usize, T, I: IO> {
    channels: FnvIndexMap<ChannelId, ChannelState<MAX_NODES>, MAX_CHANNELS>,
    storage: FnvIndexMap<ChannelId, Storage<T, I>, MAX_CHANNELS>,
    //_phantom: PhantomData<T>,
}

impl<const MAX_CHANNELS: usize, const MAX_NODES: usize, const MAX_RECORDS: usize, T, I: IO>
Client<MAX_CHANNELS, MAX_NODES, MAX_RECORDS, T, I> {
    pub fn new() -> Self {
        Self {
            channels: FnvIndexMap::new(),
            storage: FnvIndexMap::new(),
            //_phantom: PhantomData::<T>,
        }
    }

}