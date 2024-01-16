#![no_std]

use core::{marker::PhantomData, mem::size_of, ops::Deref};
use heapless::{FnvIndexMap, Vec, String};

use postcard::{from_bytes, to_slice};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

mod channel;
use channel::*;

mod storage;
use storage::*;

mod chat;
use chat::*;

mod crypto;
use crypto::*;

#[cfg(test)]
mod test;

#[derive(Debug)]
enum ClientError {
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

pub struct Channel<
const MAX_NODES: usize,
I: IO,
C: Crypto,
> {
    id: ChannelId,
    state: ChannelState<MAX_NODES, C::PubSigningKey>,
    storage: Storage<I>,
    chat: Chat<MAX_NODES, C>,
}

pub struct ClientChannels<
const MAX_CHANNELS: usize,
const MAX_NODES: usize,
I: IO,
C: Crypto,
> {
    channels: FnvIndexMap<ChannelId, Channel<MAX_NODES, I, C>, MAX_CHANNELS>,
}

impl<
const MAX_CHANNELS: usize,
const MAX_NODES: usize,
I: IO,
C: Crypto,
> ClientChannels<MAX_CHANNELS, MAX_NODES, I, C> {
    pub const fn new() -> Self {
        Self { channels: FnvIndexMap::new() }
    }
}

const MAX_SIG: usize = 256;
const MAX_ENVELOPE: usize = 1024 - MAX_SIG; 
pub struct Client<
    'a,
    'b,
    const MAX_CHANNELS: usize,
    const MAX_NODES: usize,
    I: IO,
    C: Crypto,
> {
    crypto: &'a mut C,
    node_id: NodeId,
    key_pair: KeyPair<C::PrivateSigningKey, C::PubSigningKey>,
    channels: &'b mut FnvIndexMap<ChannelId, Channel<MAX_NODES, I, C>, MAX_CHANNELS>,
}

impl<
        'a,
        'b, 
        const MAX_CHANNELS: usize,
        const MAX_NODES: usize,
        I: IO,
        C: Crypto,
    > Client<'a, 'b, MAX_CHANNELS, MAX_NODES, I, C>
{
    pub fn new(
        key_pair: KeyPair<C::PrivateSigningKey, C::PubSigningKey>,
        crypto: &'a mut C,
        channels: &'b mut ClientChannels<MAX_CHANNELS, MAX_NODES, I, C>
    ) -> Self {
        let node_id = C::compute_id(&key_pair.public);
        Self {
            crypto,
            node_id,
            key_pair,
            channels: &mut channels.channels
        }
    }

    pub const fn get_map() ->  FnvIndexMap<ChannelId, Channel<MAX_NODES, I, C>, MAX_CHANNELS> {
        FnvIndexMap::new()
    }

    pub fn send_message(&mut self, channel_id: &ChannelId, msg: &str) -> Result<(), ClientError> {
        let Ok(text) = String::try_from(msg) else {
            return Err(ClientError::MessageToLarge);
        };

        let data: Protocol<C::PubSigningKey> = Protocol::ChatMessage(ChatMessage {
            text,
        });

        self.do_send(channel_id, data)?;

        Ok(())
    }

    pub fn add_node(&mut self, channel_id: &ChannelId, pub_key: C::PubSigningKey, name: &str) -> Result<(), ClientError> {
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

    pub fn remove_node(&mut self, channel_id: &ChannelId, node_id: &NodeId) -> Result<(), ClientError> {
        unimplemented!()
    }

    pub fn list_nodes(&self, channel_id: &ChannelId) -> Result<&[NodeSequence<C::PubSigningKey>], ClientError> {
        let channel = self.channels.get(channel_id)
            .ok_or(ClientError::UnknownChannel)?;
        Ok(channel.state.list_nodes())
    }

    pub fn open_chat(&mut self, channel_id: ChannelId, io: I) -> Result<(), ClientError> {
        let my_id = C::compute_id(&self.key_pair.public);

        let mut storage = Storage::new(io);
        let mut channel
            = ChannelState::<MAX_NODES, C::PubSigningKey>::new(
                my_id, 
                self.key_pair.public.clone())?;

        let mut chat = Chat::<MAX_NODES, C>::new(channel_id.clone());

        let start = storage.get_cursor_from(0)?;

        if let Some(mut cursor) = start {
            while let Some(found) = storage.read(cursor)? {
                let data;
                (data, cursor) = found;
                let sealed_envelope: SealedEnvelope<Protocol<C::PubSigningKey>, MAX_ENVELOPE, MAX_SIG> = from_bytes(data)?;
                let envlope_id = self.crypto.envelope_id(&sealed_envelope);
                let from = sealed_envelope.from();
                let pub_key = channel.get_node_key(from)?;
                let message = self.crypto.open(&pub_key, &sealed_envelope)?;
                
                channel.check_receive(from, &message, &envlope_id)?;
                
                let maby_key = chat.accept_message(channel_id, from, &message.data)?;
                
                if let Some(new_pub_key) = maby_key {
                    let node_id = C::compute_id(&new_pub_key);
                    channel.add_node(node_id, new_pub_key)?;
                }

                if let Err(_) = channel.receive(from, &message, &envlope_id) {
                    return Err(ClientError::Unreachable); 
                }
            }
        }

        let full_channel = Channel {
            id: channel_id.clone(),
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
            return Err(ClientError::StringTooLarge)
        };

        let message = NewChannel {
            nonce,
            name,
            owner: self.key_pair.public.clone(),
        };

        let mut target = [0; 4096]; // BUG: should we take this as an argument?

        let serlized = to_slice(&message, target.as_mut_slice())?;
        let channel_id = self.crypto.channel_id_from_bytes(serlized);

        let my_id = C::compute_id(&self.key_pair.public);
        let mut channel = ChannelState::<MAX_NODES, C::PubSigningKey>::new(my_id, self.key_pair.public.clone())?;
        let mut chat = Chat::<MAX_NODES, C>::new(channel_id.clone());
        let mut storage = Storage::new(io);

        let protocol = Protocol::NewChannel(message);

        let to = Recipient::Channel(channel_id.clone());
        let message = channel.address(my_id, protocol)?;

        // -seal envelope
        let sealed_envelope =
            self.crypto
                .seal::<_, MAX_ENVELOPE, MAX_SIG>(my_id, to, &self.key_pair, &message, &mut target)?;

        let envlope_id = self.crypto.envelope_id(&sealed_envelope);
        // -check that we can receive it
        // BUG: This actually allocates a new client
        // So there is a DOS here where and attacker
        // can send junk messages and overflow memory.
        channel.check_receive(my_id, &message, &envlope_id)?;
        // -check the message on chat
        chat.accept_message(channel_id.clone(), my_id, &message.data)?;
        // -receive it
        let max_sequance = channel.receive(my_id, &message, &envlope_id)?;
        // -store it
        let mut slab_writer = storage.get_writer()?;
        let serlized_envlope = to_slice(&sealed_envelope, target.as_mut_slice())?;
        slab_writer.write_record(max_sequance, &serlized_envlope)?;
        slab_writer.commit()?;

        let full_channel = Channel {
            id: channel_id.clone(),
            state: channel,
            storage,
            chat,
        };

        let Ok(_) = self.channels.insert(channel_id.clone(), full_channel) else {
            return Err(ClientError::ChannelLimit);
        };

        Ok(channel_id)
    }

    fn do_send(&mut self, channel_id: &ChannelId, data: Protocol<C::PubSigningKey>) -> Result<(), ClientError> {
        let from = self.node_id;
        let to = Recipient::Channel(channel_id.clone());
        let mut target = [0u8; 4096]; // BUG: should we take this as an argument?

        let channel = self.channels.get_mut(channel_id)
        .ok_or(ClientError::UnknownChannel)?;

        let message = channel.state.address(from, data)?;
        let envelope = self.crypto.seal::<_, MAX_ENVELOPE, MAX_SIG>(from, to, &self.key_pair, &message, target.as_mut_slice())?;

        let envlope_id = self.crypto.envelope_id(&envelope);
        // -check that we can receive it
        // BUG: This actually allocates a new client
        // So there is a DOS here where and attacker
        // can send junk messages and overflow memory.
        channel.state.check_receive(from, &message, &envlope_id)?;
        
        // -check the message on chat
        let result = channel.chat.accept_message(channel_id.clone(), from, &message.data)?;
        
        // -receive it
        //let max_sequance = channel.state.receive(from, &message, &envlope_id)?;
        let Ok(max_sequance) = channel.state.receive(from, &message, &envlope_id) else {
            return Err(ClientError::Unreachable); 
        };

        // - store the pub key for later
        if let Some(new_pub_key) = result {
            let node_id = C::compute_id(&new_pub_key);
            channel.state.add_node(node_id, new_pub_key)?;
        }

        // -store it
        let mut slab_writer = channel.storage.get_writer()?;
        let serlized_envlope = to_slice(&envelope, target.as_mut_slice())?;
        slab_writer.write_record(max_sequance, &serlized_envlope)?;
        slab_writer.commit()?;

        Ok(())
    }

}
