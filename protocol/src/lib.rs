#![no_std]

use core::{marker::PhantomData, mem::size_of};
use heapless::{FnvIndexMap, String, Vec};

use postcard::{from_bytes, to_slice};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub mod channel;
use channel::*;

pub mod storage;
use storage::*;

pub mod chat;
use chat::*;

pub mod crypto;
use crypto::*;

pub mod heap_type;
use heap_type::*;

pub mod sync;
use sync::*;

pub mod wire;

#[cfg(test)]
mod test;

#[derive(Debug)]
pub enum ClientError {
    SerializationError(postcard::Error),
    ChannelError(ChannelError),
    CryptoError(CryptoError),
    ChatError(ChatError),
    StorageError(StorageError),
    ChannelLimit,
    Unreachable,
    StringTooLarge,
    UnknownChannel,
    MessageToLarge,
    SafeStaticError,
    MessageIndexOutOfBounds,
}

impl From<GuardCellError> for ClientError {
    fn from(_value: GuardCellError) -> Self {
        ClientError::SafeStaticError
    }
}

impl From<postcard::Error> for ClientError {
    fn from(value: postcard::Error) -> ClientError {
        ClientError::SerializationError(value)
    }
}

impl From<ChannelError> for ClientError {
    fn from(value: ChannelError) -> Self {
        ClientError::ChannelError(value)
    }
}

impl From<CryptoError> for ClientError {
    fn from(value: CryptoError) -> Self {
        ClientError::CryptoError(value)
    }
}

impl From<ChatError> for ClientError {
    fn from(value: ChatError) -> Self {
        ClientError::ChatError(value)
    }
}

impl From<StorageError> for ClientError {
    fn from(value: StorageError) -> Self {
        ClientError::StorageError(value)
    }
}

pub struct Channel<const MAX_NODES: usize, I: IO, C: Crypto> {
    state: ChannelState<MAX_NODES, C::PubSigningKey>,
    storage: Storage<I>,
    chat: Chat<MAX_NODES, C>,
}

pub struct ClientChannels<const MAX_CHANNELS: usize, const MAX_NODES: usize, I: IO, C: Crypto> {
    channels: FnvIndexMap<ChannelId, Channel<MAX_NODES, I, C>, MAX_CHANNELS>,
}

impl<const MAX_CHANNELS: usize, const MAX_NODES: usize, I: IO, C: Crypto>
    ClientChannels<MAX_CHANNELS, MAX_NODES, I, C>
{
    pub const fn new() -> Self {
        Self {
            channels: FnvIndexMap::new(),
        }
    }
}

const MAX_SIG: usize = 256;
const MAX_ENVELOPE: usize = 1024 - MAX_SIG;
const LEN_SIZE: usize = size_of::<u32>();

pub struct Client<'a, 'b, const MAX_CHANNELS: usize, const MAX_NODES: usize, I: IO, C: Crypto> {
    crypto: &'a mut C,
    node_id: NodeId,
    key_pair: KeyPair<C::PrivateSigningKey, C::PubSigningKey>,
    channels: &'b mut FnvIndexMap<ChannelId, Channel<MAX_NODES, I, C>, MAX_CHANNELS>,
}

impl<'a, 'b, const MAX_CHANNELS: usize, const MAX_NODES: usize, I: IO, C: Crypto>
    Client<'a, 'b, MAX_CHANNELS, MAX_NODES, I, C>
{
    pub fn new(
        key_pair: KeyPair<C::PrivateSigningKey, C::PubSigningKey>,
        crypto: &'a mut C,
        channels: &'b mut ClientChannels<MAX_CHANNELS, MAX_NODES, I, C>,
    ) -> Self {
        let node_id = C::compute_id(&key_pair.public);
        Self {
            crypto,
            node_id,
            key_pair,
            channels: &mut channels.channels,
        }
    }

    pub fn get_pub_key(&self) -> C::PubSigningKey {
        self.key_pair.public.clone()
    }

    pub fn get_node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn finish_sync_request(
        &self,
        channel_id: &ChannelId,
        request: &mut SyncRequest<MAX_NODES>,
    ) -> Result<(), ClientError> {
        let channel = self
            .channels
            .get(channel_id)
            .ok_or(ClientError::UnknownChannel)?;
        let nodes = channel.state.list_nodes();

        request.vector_clock.truncate(0);

        for node in nodes {
            let clock = Clock::new(node.node, node.sequence);
            request.vector_clock.push(clock)
                .map_err(|_| ClientError::Unreachable)?;
        }

        Ok(())
    }

    // we fill out the SyncResponderSate's vector clock by
    // merging the nodes and their sequences. If they are missing
    // and node set it's sequence to the nodes first sequence.
    //
    // BUG: I was irrupted a bunch writing this it needs to reviewed
    pub fn start_sync_response(
        &self,
        channel_id: &ChannelId,
        state: &mut SyncResponderState<MAX_NODES>,
        request: &SyncRequest<MAX_NODES>,
    ) -> Result<(), ClientError> {
        let channel = self
            .channels
            .get(channel_id)
            .ok_or(ClientError::UnknownChannel)?;

        // Set the SyncResponderState vector clock to be values from the
        // request or 0 if request is missing a node
        state.vector_clock.truncate(0);

        let mut request_index = 0;

        'outer: for my_node in channel.state.list_nodes() {
            while let Some(request_node) = request.vector_clock.get(request_index) {
                if request_node.node == my_node.node {
                    // if they are equal advance both clocks
                    let clock = Clock::new(request_node.node, request_node.sequence);
                    state
                        .vector_clock
                        .push(clock)
                        .or(Err(ClientError::Unreachable))?;

                    // Safe because MAX_NODE < usize MAX
                    request_index += 1;

                    // We need to return to the outer loop to advance
                    // to the next node there.
                    continue 'outer;
                } else if request_node.node < my_node.node {
                    // if request is smaller add it's clock
                    let clock = Clock::new(request_node.node, request_node.sequence);
                    state
                        .vector_clock
                        .push(clock)
                        .or(Err(ClientError::Unreachable))?;

                    // Safe because MAX_NODE < usize MAX
                    request_index += 1;
                } else {
                    // Handle this case in the outer loop
                    // to handel the case where we have exhausted
                    // request vector
                    break;
                }
            }

            // If we are here that means they don't have the node
            let clock = Clock::new(my_node.node, my_node.first_sequence);
            state
                .vector_clock
                .push(clock)
                .or(Err(ClientError::Unreachable))?;
        }

        // If there are any request clocks left add them to the end.
        while let Some(request_node) = request.vector_clock.get(request_index) {
            let clock = Clock::new(request_node.node, request_node.sequence);
            state
                .vector_clock
                .push(clock)
                .or(Err(ClientError::Unreachable))?;

            // Safe because MAX_NODE < usize MAX
            request_index += 1;
        }

        Ok(())
    }

    pub fn fill_send_buffer(
        &self,
        channel_id: &ChannelId,
        state: &mut SyncResponderState<MAX_NODES>,
        buffer: &mut [u8],
    ) -> Result<(u32, usize), ClientError> {
        let channel = self
            .channels
            .get(channel_id)
            .ok_or(ClientError::UnknownChannel)?;

        let start = state.get_min_sequence().ok_or(ClientError::Unreachable)?;

        let mut cursor = channel
            .storage
            .get_cursor_from_sequence(start)?
            .ok_or(ClientError::Unreachable)?;

        let mut offset = 0;
        let mut count = 0;

        while let Some((data, next)) = channel.storage.read(cursor)? {
            // BUG: need to see if this is a message they need and update the `state`
            // Right now this will send them things they may not need

            if (buffer.len() - offset) < (data.len() + LEN_SIZE) {
                break;
            }

            let len = data.len() as u32;
            offset = write_u32(len, buffer, offset)?;

            let target = buffer
                .get_mut(offset..(offset + data.len()))
                .ok_or(ClientError::Unreachable)?;

            target.copy_from_slice(data);

            offset += data.len();
            count += 1;
            cursor = next;
        }

        Ok((count, offset))
    }

    pub fn receive_buffer(
        &mut self,
        channel_id: &ChannelId,
        buffer: &[u8],
        count: u32,
    ) -> Result<(), ClientError> {
        let mut offset = 0;
        let buffer = buffer;
        for _ in 0..count {
            let len: u32;
            (len, offset) = read_u32(buffer, offset)?;
            let end = offset + len as usize;
            let envelope_bytes = buffer
                .get(offset..end)
                // BUG: this is not unreachable but I don't have the right
                // error and I think all this code should move in the the sync mod.
                .ok_or(ClientError::Unreachable)?;
            offset = end;
            match self.do_receive(channel_id, envelope_bytes) {
                Ok(_) => (),
                Err(ClientError::ChannelError(ChannelError::AlreadyReceived)) => (),
                Err(err) => return Err(err),
            }
        }

        Ok(())
    }

    pub fn message_count(&self, channel_id: &ChannelId) -> Result<u64, ClientError> {
        let channel = self
            .channels
            .get(channel_id)
            .ok_or(ClientError::UnknownChannel)?;

        Ok(channel.chat.message_count())
    }

    pub fn send_message(&mut self, channel_id: &ChannelId, msg: &str) -> Result<(), ClientError> {
        let Ok(text) = String::try_from(msg) else {
            return Err(ClientError::MessageToLarge);
        };

        let data: Protocol<C::PubSigningKey> = Protocol::ChatMessage(ChatMessage { text });

        self.do_send(channel_id, data)?;

        Ok(())
    }

    pub fn get_message<'c>(
        &'c self,
        channel_id: &ChannelId,
        index: u64,
    ) -> Result<ChatMessage, ClientError> {
        let channel = self
            .channels
            .get(channel_id)
            .ok_or(ClientError::UnknownChannel)?;

        let cursor = channel
            .storage
            .get_cursor_from_index(index)?
            .ok_or(ClientError::MessageIndexOutOfBounds)?;

        let (bytes, _cursor) = channel
            .storage
            .read(cursor)?
            .ok_or(ClientError::Unreachable)?;

        let envelope: SealedEnvelope<Protocol<C::PubSigningKey>, MAX_ENVELOPE, MAX_SIG> =
            from_bytes(bytes)?;
        let key = channel.state.get_node_key(envelope.from)?;
        let message = self.crypto.open(&key, &envelope)?;
        let Protocol::ChatMessage(message) = message.data else {
            return Err(ClientError::Unreachable);
        };

        Ok(message)
    }

    pub fn add_node(
        &mut self,
        channel_id: &ChannelId,
        pub_key: C::PubSigningKey,
        name: &str,
    ) -> Result<(), ClientError> {
        let Ok(name_string) = String::try_from(name) else {
            return Err(ClientError::MessageToLarge);
        };

        let data: Protocol<C::PubSigningKey> = Protocol::AddUser(AddUser {
            name: name_string,
            key: pub_key,
        });

        self.do_send(channel_id, data)?;

        Ok(())
    }

    pub fn remove_node(
        &mut self,
        _channel_id: &ChannelId,
        _node_id: &NodeId,
    ) -> Result<(), ClientError> {
        unimplemented!()
    }

    pub fn list_nodes(
        &self,
        channel_id: &ChannelId,
    ) -> Result<&[NodeSequence<C::PubSigningKey>], ClientError> {
        let channel = self
            .channels
            .get(channel_id)
            .ok_or(ClientError::UnknownChannel)?;
        Ok(channel.state.list_nodes())
    }

    pub fn open_chat(&mut self, channel_id: ChannelId, io: I) -> Result<(), ClientError> {
        let my_id = C::compute_id(&self.key_pair.public);

        let storage = Storage::new(io);
        let mut channel =
            ChannelState::<MAX_NODES, C::PubSigningKey>::new(my_id, self.key_pair.public.clone())?;

        let mut chat = Chat::<MAX_NODES, C>::new(channel_id.clone());

        let start = storage.get_cursor_from_sequence(0)?;

        if let Some(mut cursor) = start {
            while let Some(found) = storage.read(cursor)? {
                let data;
                (data, cursor) = found;
                let sealed_envelope: SealedEnvelope<
                    Protocol<C::PubSigningKey>,
                    MAX_ENVELOPE,
                    MAX_SIG,
                > = from_bytes(data)?;
                let envelope_id = self.crypto.envelope_id(&sealed_envelope);
                let from = sealed_envelope.from();
                let pub_key = channel.get_node_key(from)?;
                let message = self.crypto.open(&pub_key, &sealed_envelope)?;

                channel.check_receive(from, &message, &envelope_id)?;

                let accept_result = chat.accept_message(channel_id, from, &message.data)?;

                if let AcceptResult::AddUser(new_pub_key) = accept_result {
                    let node_id = C::compute_id(&new_pub_key);
                    channel.add_node(node_id, new_pub_key)?;
                }

                if let Err(_) = channel.receive(from, &message, &envelope_id) {
                    return Err(ClientError::Unreachable);
                }
            }
        }

        let full_channel = Channel {
            state: channel,
            storage,
            chat,
        };

        let Ok(_) = self.channels.insert(channel_id.clone(), full_channel) else {
            return Err(ClientError::ChannelLimit);
        };

        Ok(())
    }

    pub fn init_chat(&mut self, name_str: &str, io: I) -> Result<ChannelId, ClientError> {
        let nonce = self.crypto.nonce();

        let Ok(name) = name_str.try_into() else {
            return Err(ClientError::StringTooLarge);
        };

        let message = NewChannel {
            nonce,
            name,
            owner: self.key_pair.public.clone(),
        };

        let mut target = [0; 4096]; // BUG: should we take this as an argument?

        let serialized = to_slice(&message, target.as_mut_slice())?;
        let channel_id = self.crypto.channel_id_from_bytes(serialized);

        let my_id = C::compute_id(&self.key_pair.public);
        let mut channel =
            ChannelState::<MAX_NODES, C::PubSigningKey>::new(my_id, self.key_pair.public.clone())?;
        let mut chat = Chat::<MAX_NODES, C>::new(channel_id.clone());
        let mut storage = Storage::new(io);

        let protocol = Protocol::NewChannel(message);

        let to = Recipient::Channel(channel_id.clone());
        let message = channel.address(my_id, protocol)?;

        let sequence = message.sequence();

        // -seal envelope
        let sealed_envelope = self.crypto.seal::<_, MAX_ENVELOPE, MAX_SIG>(
            my_id,
            to,
            &self.key_pair,
            &message,
            &mut target,
        )?;

        let envelope_id = self.crypto.envelope_id(&sealed_envelope);
        // -check that we can receive it
        // BUG: This actually allocates a new client
        // So there is a DOS here where and attacker
        // can send junk messages and overflow memory.
        channel.check_receive(my_id, &message, &envelope_id)?;
        // -check the message on chat
        chat.accept_message(channel_id.clone(), my_id, &message.data)?;
        // -receive it
        let max_sequence = channel.receive(my_id, &message, &envelope_id)?;
        // -store it
        let message_count = chat.message_count();
        let mut slab_writer = storage.get_writer()?;
        let serialized_envelope = to_slice(&sealed_envelope, target.as_mut_slice())?;
        slab_writer.write_record(max_sequence, message_count, sequence, my_id, &serialized_envelope)?;
        slab_writer.commit()?;

        let full_channel = Channel {
            state: channel,
            storage,
            chat,
        };

        let Ok(_) = self.channels.insert(channel_id.clone(), full_channel) else {
            return Err(ClientError::ChannelLimit);
        };

        Ok(channel_id)
    }

    pub fn add_channel(&mut self, from: C::PubSigningKey, channel_id: ChannelId, io: I) ->  Result<(), ClientError>  {
        let my_id = C::compute_id(&from);
        let channel =
            ChannelState::<MAX_NODES, C::PubSigningKey>::new(my_id, from)?;
        let chat = Chat::<MAX_NODES, C>::new(channel_id.clone());
        let storage = Storage::new(io);

        let full_channel = Channel {
            state: channel,
            storage,
            chat,
        };

        let Ok(_) = self.channels.insert(channel_id.clone(), full_channel) else {
            return Err(ClientError::ChannelLimit);
        };

        Ok(())
    }


    fn do_send(
        &mut self,
        channel_id: &ChannelId,
        data: Protocol<C::PubSigningKey>,
    ) -> Result<(), ClientError> {
        let from = self.node_id;
        let to = Recipient::Channel(channel_id.clone());
        let mut target = [0u8; 4096]; // BUG: should we take this as an argument?

        let channel = self
            .channels
            .get_mut(channel_id)
            .ok_or(ClientError::UnknownChannel)?;

        let message = channel.state.address(from, data)?;
        let sequence = message.sequence();
        let envelope = self.crypto.seal::<_, MAX_ENVELOPE, MAX_SIG>(
            from,
            to,
            &self.key_pair,
            &message,
            target.as_mut_slice(),
        )?;

        let envelope_id = self.crypto.envelope_id(&envelope);
        // -check that we can receive it
        // BUG: This actually allocates a new client
        // So there is a DOS here where and attacker
        // can send junk messages and overflow memory.
        channel.state.check_receive(from, &message, &envelope_id)?;

        // -check the message on chat
        let result = channel
            .chat
            .accept_message(channel_id.clone(), from, &message.data)?;

        // -receive it
        //let max_sequence = channel.state.receive(from, &message, &envelope_id)?;
        let Ok(max_sequence) = channel.state.receive(from, &message, &envelope_id) else {
            return Err(ClientError::Unreachable);
        };

        // - store the pub key for later
        if let AcceptResult::AddUser(new_pub_key) = result {
            let node_id = C::compute_id(&new_pub_key);
            channel.state.add_node(node_id, new_pub_key)?;
        }

        // -store it
        let message_count = channel.chat.message_count();
        let mut slab_writer = channel.storage.get_writer()?;
        let serialized_envelope = to_slice(&envelope, target.as_mut_slice())?;

        slab_writer.write_record(max_sequence, message_count, sequence, from, &serialized_envelope)?;
        slab_writer.commit()?;

        Ok(())
    }

    fn do_receive(&mut self, channel_id: &ChannelId, bytes: &[u8]) -> Result<(), ClientError> {
        let sealed_envelope: SealedEnvelope<Protocol<C::PubSigningKey>, MAX_ENVELOPE, MAX_SIG> =
            from_bytes(bytes)?;

        let from = sealed_envelope.from();
        let channel = self
            .channels
            .get_mut(channel_id)
            .ok_or(ClientError::UnknownChannel)?;
        let key = channel.state.get_node_key(from)?;

        let message = self.crypto.open(&key, &sealed_envelope)?;
        let sequence = message.sequence();

        let envelope_id = self.crypto.envelope_id(&sealed_envelope);
        // -check that we can receive it
        // BUG: This actually allocates a new client
        // So there is a DOS here where and attacker
        // can send junk messages and overflow memory.
        channel.state.check_receive(from, &message, &envelope_id)?;
        // -check the message on chat
        let result = channel
            .chat
            .accept_message(channel_id.clone(), from, &message.data)?;

        // -receive it
        //let max_sequence = channel.state.receive(from, &message, &envelope_id)?;
        let Ok(max_sequence) = channel.state.receive(from, &message, &envelope_id) else {
            return Err(ClientError::Unreachable);
        };
        // - store the pub key for later
        if let AcceptResult::AddUser(new_pub_key) = result {
            let node_id = C::compute_id(&new_pub_key);
            channel.state.add_node(node_id, new_pub_key)?;
        }
        // -store it
        let message_count = channel.chat.message_count();
        let mut slab_writer = channel.storage.get_writer()?;

        slab_writer.write_record(max_sequence, message_count, sequence, from, &bytes)?;
        slab_writer.commit()?;

        Ok(())
    }
}
