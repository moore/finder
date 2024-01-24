#[cfg(test)]
extern crate std;
use std::collections::HashMap;
use std::string::String;
use std::fs::read_to_string;
use std::vec::{self, Vec};
use std::format;
use std::boxed::Box;

use serde_yaml;

use rsa::RsaPrivateKey;
use rsa::RsaPublicKey;
use rsa::pkcs1::DecodeRsaPrivateKey;

use core::mem;

use super::*;
use crate::crypto::{NodeId, ChannelId};


const MEGA_BYTE: usize = 1024 * 1024;
const SLAB_SIZE: usize = 1024;
const MAX_CHANNELS: usize = 4;
const MAX_NODES: usize = 128;


#[derive(Debug, Deserialize)]
enum TestCommands {
    NewClient { id: u64, key: String },
    NewChannel { id: u64, from: u64 },
    SendMessage { channel: u64, from: u64, text: String },
    AddClient {channel: u64, from: u64, client: u64 },
    Sync {channel: u64, requester: u64, responder: u64},
    CheckMessageCount { channel: u64, from: u64, count: u64 },
}

pub struct TestRunner {
    client_id_map: HashMap<u64, NodeId>,
    channel_id_map: HashMap<u64, ChannelId>,
    clients: HashMap<u64, &'static mut Client<'static, 'static, MAX_CHANNELS, MAX_NODES, MemIO<'static, SLAB_SIZE>, RustCrypto>>,
}

impl TestRunner {

    pub fn new() -> Self {
        Self {
            client_id_map: HashMap::new(),
            channel_id_map: HashMap::new(),
            clients: HashMap::new(),
        }
    }

    pub fn run(&mut self, file: &str) -> Result<(), ClientError> {
        let yaml = read_to_string( format!("src/test/{}", file))
            .expect("Could to read test file");
        let commands: Vec<TestCommands> = serde_yaml::from_str(&yaml)
            .expect("could not parse test yaml");


        for command in commands {
            use TestCommands::*;
            match command {
                NewClient { id, key } => 
                    self.new_client(id, key)?,
                NewChannel { id, from } => 
                    self.new_channel(id, from)?,
                SendMessage { channel, from, text } => (),
                AddClient { channel, from, client } => (),
                Sync { channel, requester, responder } => (),
                CheckMessageCount { channel, from, count } => (),
            };
        }

        Ok(())
    }

    fn new_client(&mut self, client_id: u64, key: String ) -> Result<(), ClientError> {
        let pem = read_to_string(format!("src/test/{}", key))
            .expect("could not read key");

        let seed = [0; 128];
        let  crypto = into_mut(Box::new(RustCrypto::new(&seed)?));
        let key_pair = get_test_keys(pem);

        let channels = into_mut(Box::new(ClientChannels::new()));

        let client: &mut Client<'_, '_, MAX_CHANNELS, MAX_NODES, MemIO<'_, SLAB_SIZE>, RustCrypto>
            = into_mut(Box::new(Client::new(key_pair, crypto, channels)));

        self.clients.insert(client_id, client);
        Ok(())
    }

    fn new_channel(&mut self, channel_id: u64, from: u64) -> Result<(), ClientError> {
        let client = self.clients.get_mut(&from)
            .expect("could not get client");


        // This is a dance to allocate the buffer on the heap
        // instead of allocating on the stack and moving to the heap
        let mut vec_data = Vec::with_capacity(MEGA_BYTE);
        vec_data.resize(MEGA_BYTE, 0u8);
        let boxed_data: Box<[u8; MEGA_BYTE]> = vec_data.into_boxed_slice().try_into().unwrap();
        let data = into_mut(boxed_data);
        let io: MemIO<'_, SLAB_SIZE> = MemIO::new(data)?;

        let name_str = "Test Chat";
        let channel_id_real = client.init_chat(name_str, io)?;

        self.channel_id_map.insert(channel_id, channel_id_real);

        Ok(())
    }

}




pub fn get_test_keys(pem: String) -> KeyPair<RsaPrivateKey, RsaPublicKey> {
    let private = RsaPrivateKey::from_pkcs1_pem(&pem)
        .expect("error reading key");
    let public = private.to_public_key();
    KeyPair {
        private,
        public,
    }
}

fn into_mut<T>(mut data: Box<T>) -> &'static mut T {
    let src = data.as_mut();
    let result = unsafe {
        mem::transmute::<&mut T, &'static mut T>(src)
    };
    mem::forget(data);
    result

}