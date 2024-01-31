#[cfg(test)]
extern crate std;
use std::boxed::Box;
use std::collections::HashMap;
use std::format;
use std::fs::read_to_string;
use std::string::String;
use std::vec::Vec;

use serde_yaml;

use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::RsaPrivateKey;
use rsa::RsaPublicKey;

use core::mem;

use super::*;
use crate::crypto::{ChannelId, NodeId};

const MEGA_BYTE: usize = 1024 * 1024;
const SLAB_SIZE: usize = 1024;
const MAX_CHANNELS: usize = 4;
const MAX_NODES: usize = 128;
const RESPONSE_MAX: usize = 4096;


#[derive(Debug, Deserialize)]
enum TestCommands {
    NewClient {
        id: u64,
        key: String,
    },
    NewChannel {
        id: u64,
        from: u64,
    },
    SendMessage {
        channel: u64,
        from: u64,
        text: String,
    },
    AddClient {
        channel: u64,
        from: u64,
        client: u64,
    },
    Sync {
        channel: u64,
        requester: u64,
        responder: u64,
    },
    CheckMessageCount {
        channel: u64,
        from: u64,
        count: u64,
    },
}

pub struct TestRunner {
    client_id_map: HashMap<u64, NodeId>,
    channel_id_map: HashMap<u64, ChannelId>,
    clients: HashMap<
        u64,
        &'static mut Client<
            'static,
            'static,
            MAX_CHANNELS,
            MAX_NODES,
            MemIO<'static, SLAB_SIZE>,
            RustCrypto,
        >,
    >,
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
        let yaml = read_to_string(format!("src/test/{}", file)).expect("Could to read test file");
        let commands: Vec<TestCommands> =
            serde_yaml::from_str(&yaml).expect("could not parse test yaml");

        for command in commands {
            use TestCommands::*;
            match command {
                NewClient { id, key } => self.new_client(id, key)?,
                NewChannel { id, from } => self.new_channel(id, from)?,
                SendMessage {
                    channel,
                    from,
                    text,
                } => self.send_message(channel, from, text)?,
                AddClient {
                    channel,
                    from,
                    client,
                } => self.add_client(channel, from, client)?,
                Sync {
                    channel,
                    requester,
                    responder,
                } => self.sync(channel, requester, responder)?,
                CheckMessageCount {
                    channel,
                    from,
                    count,
                } => self.check_message_count(channel, from, count)?,
            };
        }

        Ok(())
    }

    fn new_client(&mut self, client_id: u64, key: String) -> Result<(), ClientError> {
        let pem = read_to_string(format!("src/test/{}", key)).expect("could not read key");

        let seed = [0; 128];
        let crypto = into_mut(Box::new(RustCrypto::new(&seed)?));
        let key_pair = get_test_keys(pem);

        let channels = into_mut(Box::new(ClientChannels::new()));

        let client: &mut Client<'_, '_, MAX_CHANNELS, MAX_NODES, MemIO<'_, SLAB_SIZE>, RustCrypto> =
            into_mut(Box::new(Client::new(key_pair, crypto, channels)));

        self.clients.insert(client_id, client);
        Ok(())
    }

    fn new_channel(&mut self, channel_id: u64, from: u64) -> Result<(), ClientError> {
        let client = self.clients.get_mut(&from).expect("could not get client");

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

    fn send_message(&mut self, channel_id: u64, from: u64, text: String) -> Result<(), ClientError> {
        let client = self.clients.get_mut(&from).expect("could not get client");
        let channel_id_real = self.channel_id_map.get(&channel_id)
            .expect("no such channel");

        client.send_message(channel_id_real, text.as_str())?;

        Ok(())
    }

    fn add_client(&mut self, channel_id: u64, from: u64, to_add: u64) -> Result<(), ClientError> {
        let channel_id_real = self.channel_id_map.get(&channel_id)
            .expect("no such channel");
    
        let client = self.clients.get_mut(&from)
            .expect("could not get client");

        // BUG: this is wrong if there are users
        // other than the owner who can add clients
        let owner_key = client.get_pub_key();


        let to_add = self.clients.get_mut(&to_add)
            .expect("could not get client to add");

        let pub_key = to_add.get_pub_key();

        // This is a dance to allocate the buffer on the heap
        // instead of allocating on the stack and moving to the heap
        let mut vec_data = Vec::with_capacity(MEGA_BYTE);
        vec_data.resize(MEGA_BYTE, 0u8);
        let boxed_data: Box<[u8; MEGA_BYTE]> = vec_data.into_boxed_slice().try_into().unwrap();
        let data = into_mut(boxed_data);
        let io: MemIO<'_, SLAB_SIZE> = MemIO::new(data)?;
        
        to_add.add_channel(owner_key, channel_id_real.clone(), io)?;
        
        let client = self.clients.get_mut(&from)
            .expect("could not get client");
       
       
        client.add_node(channel_id_real, pub_key, "It's a name")?;

        Ok(())
    }

    fn sync(&mut self, channel_id: u64, requester: u64, responder: u64) -> Result<(), ClientError> {
        let channel_id_real = self.channel_id_map.get(&channel_id)
            .expect("no such channel");
        
        let client = self.clients.get_mut(&requester)
            .expect("could not get request client");
        
        let mut request = SyncRequest::<MAX_NODES> {
            session_id: 0,
            bytes_budget: 4096,
            vector_clock: heapless::Vec::new(),
        };

        client.finish_sync_request(&channel_id_real, &mut request)
            .expect("Could not finish request");

        ///// send request over network

        let client = self.clients.get_mut(&responder)
        .expect("could not get response client");

        let mut response_state = SyncResponderState::new(&request);

        client.start_sync_response(&channel_id_real, &mut response_state, &request)
            .expect("could not start_sync_response");

        loop {
            let client = self.clients.get_mut(&responder)
                .expect("could not get response client");

            let mut response = SyncResponse::<RESPONSE_MAX>::new(request.session_id);
            let buffer = response.data.as_mut();
            let (count, offset) = client.fill_send_buffer(&channel_id_real, &mut response_state, buffer)
                .expect("Could not fill buffer");
            if count == 0 {
                break;
            }

            response.data.truncate(offset);
            response.count = count;

            ///// this is where the network transfer would happen

            let client = self.clients.get_mut(&requester)
                .expect("could not get request client");

            let buffer = response.data.as_ref();
            let count = response.count;
            client.receive_buffer(&channel_id_real, buffer, count)
                .expect("Could not receive buffer");

            break; //BOOG
        }
        Ok(())
    }


    fn check_message_count(&mut self, channel_id: u64, from: u64, expected: u64) -> Result<(), ClientError> {
        let client = self.clients.get_mut(&from)
            .expect("could not get client");
        let channel_id_real = self.channel_id_map.get(&channel_id)
            .expect("no such channel");

        let count = client.message_count(channel_id_real)?;

        assert_eq!(count, expected);

        Ok(())
    }
}

pub fn get_test_keys(pem: String) -> KeyPair<RsaPrivateKey, RsaPublicKey> {
    let private = RsaPrivateKey::from_pkcs1_pem(&pem).expect("error reading key");
    let public = private.to_public_key();
    KeyPair { private, public }
}

pub fn into_mut<T>(mut data: Box<T>) -> &'static mut T {
    let src = data.as_mut();
    let result = unsafe { mem::transmute::<&mut T, &'static mut T>(src) };
    mem::forget(data);
    result
}
