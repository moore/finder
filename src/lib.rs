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


const MAX_SIG: usize = 256;
const MAX_ENVELOPE: usize = 1024 - MAX_SIG; 
pub struct Client<
    'a,
    const MAX_CHANNELS: usize,
    const MAX_NODES: usize,
    I: IO,
    C: Crypto,
> {
    crypto: &'a mut C,
    key_pair: KeyPair<C::PrivateSigningKey, C::PubSigningKey>,
    channels: FnvIndexMap<ChannelId, ChannelState<MAX_NODES, C::PubSigningKey>, MAX_CHANNELS>,
    storage: FnvIndexMap<ChannelId, Storage<I>, MAX_CHANNELS>,
    chats: FnvIndexMap<ChannelId, Chat<MAX_NODES, C>, MAX_CHANNELS>,
}

impl<
        'a,
        const MAX_CHANNELS: usize,
        const MAX_NODES: usize,
        I: IO,
        C: Crypto,
    > Client<'a, MAX_CHANNELS, MAX_NODES, I, C>
{
    pub fn new(
        key_pair: KeyPair<C::PrivateSigningKey, C::PubSigningKey>,
        crypto: &'a mut C,
    ) -> Self {
        Self {
            crypto,
            key_pair,
            channels: FnvIndexMap::new(),
            storage: FnvIndexMap::new(),
            chats: FnvIndexMap::new(),
        }
    }

    pub fn send_message(&mut self, test: &str) -> Result<(), ClientError> {
        unimplemented!()
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
            }
        }

        let Ok(_) = self.channels.insert(channel_id.clone(), channel) else {
            return Err(ClientError::ChannelLimit);
        };

        let Ok(_) = self.storage.insert(channel_id.clone(), storage) else {
            return Err(ClientError::Unreachable);
        };

        let Ok(_) = self.chats.insert(channel_id.clone(), chat) else {
            return Err(ClientError::Unreachable);
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

        let Ok(_) = self.channels.insert(channel_id.clone(), channel) else {
            return Err(ClientError::ChannelLimit);
        };

        let Ok(_) = self.storage.insert(channel_id.clone(), storage) else {
            return Err(ClientError::Unreachable);
        };

        let Ok(_) = self.chats.insert(channel_id.clone(), chat) else {
            return Err(ClientError::Unreachable);
        };

        Ok(channel_id)
    }
}
