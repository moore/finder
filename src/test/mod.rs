
use super::*;

use crypto::rust::{RustCrypto, test::get_test_keys};
use storage::mem_io::MemIO;

const MEGA_BYTE: usize = 1024 * 1024;
const SLAB_SIZE: usize = 1024;
const MAX_CHANNELS: usize = 4;
const MAX_NODES: usize = 128;

#[test]
fn test_init_chat() -> Result<(), ClientError> {
    let seed = [0; 128];
    let mut crypto = RustCrypto::new(&seed)?;
    let key_pair = get_test_keys();
    static buffer: GuardCell<[u8 ; MEGA_BYTE]> = GuardCell::wrap([0u8; MEGA_BYTE]);
    let data = buffer.take_mut()?;
    let io: MemIO<'_, SLAB_SIZE> = MemIO::new(data)?;

    //let mut client: Client<'_, MAX_CHANNELS, MAX_NODES, MemIO<'_, SLAB_SIZE>, RustCrypto> 
    //    = Client::new(key_pair, &mut crypto);

    static channels_const: GuardCell<ClientChannels<MAX_CHANNELS, MAX_NODES, MemIO<'_, SLAB_SIZE>, RustCrypto>> 
        = GuardCell::wrap(ClientChannels::new());

    let channels = channels_const.take_mut()?;


    let mut client: Client<'_, '_, MAX_CHANNELS, MAX_NODES, MemIO<'_, SLAB_SIZE>, RustCrypto> 
        = Client::new(key_pair, &mut crypto, channels);

    let name_str = "Test Chat";
    let channel_id = client.init_chat(name_str, io)?;
    let nodes = client.list_nodes(&channel_id)?;
    assert_eq!(nodes.len(), 1);
    Ok(())
}

#[test]
fn test_open_chat() -> Result<(), ClientError> {
    static seed: [u8 ; 128] = [0u8; 128];
    let mut crypto = RustCrypto::new(&seed)?;
    let key_pair = get_test_keys();
    

    let channel_id = {
        static buffer: GuardCell<[u8 ; MEGA_BYTE]> = GuardCell::wrap([0u8; MEGA_BYTE]);
        let data = buffer.take_mut()?;
        let io: MemIO<'_, SLAB_SIZE> = MemIO::new(data)?;

        static channels_const: GuardCell<ClientChannels<MAX_CHANNELS, MAX_NODES, MemIO<'_, SLAB_SIZE>, RustCrypto>> 
        = GuardCell::wrap(ClientChannels::new());

        let channels = channels_const.take_mut()?;


        let mut client: Client<'_, '_, MAX_CHANNELS, MAX_NODES, MemIO<'_, SLAB_SIZE>, RustCrypto> 
            = Client::new(key_pair.clone(), &mut crypto, channels);

        let name_str = "Test Chat";
        client.init_chat(name_str, io)?
    };

    static buffer: GuardCell<[u8 ; MEGA_BYTE]> = GuardCell::wrap([0u8; MEGA_BYTE]);
    let data = buffer.take_mut()?;
    let io: MemIO<'_, SLAB_SIZE> = MemIO::new(data)?;

    static channels_const: GuardCell<ClientChannels<MAX_CHANNELS, MAX_NODES, MemIO<'_, SLAB_SIZE>, RustCrypto>> 
    = GuardCell::wrap(ClientChannels::new());

    let channels = channels_const.take_mut()?;


    let mut client: Client<'_, '_, MAX_CHANNELS, MAX_NODES, MemIO<'_, SLAB_SIZE>, RustCrypto> 
        = Client::new(key_pair, &mut crypto, channels);

    client.open_chat(channel_id, io)?;

    let nodes = client.list_nodes(&channel_id)?;
    assert_eq!(nodes.len(), 1);

    Ok(())
}

#[test]
fn test_send_message() -> Result<(), ClientError> {
    let seed = [0; 128];
    let mut crypto = RustCrypto::new(&seed)?;
    let key_pair = get_test_keys();
    static buffer: GuardCell<[u8 ; MEGA_BYTE]> = GuardCell::wrap([0u8; MEGA_BYTE]);
    let data = buffer.take_mut()?;
    let io: MemIO<'_, SLAB_SIZE> = MemIO::new(data)?;

    static channels_const: GuardCell<ClientChannels<MAX_CHANNELS, MAX_NODES, MemIO<'_, SLAB_SIZE>, RustCrypto>> 
        = GuardCell::wrap(ClientChannels::new());
    
    let channels = channels_const.take_mut()?;

    let mut client: Client<'_, '_, MAX_CHANNELS, MAX_NODES, MemIO<'_, SLAB_SIZE>, RustCrypto> 
        = Client::new(key_pair, &mut crypto, channels);

    let name_str = "Test Chat";
    let channel_id = client.init_chat(name_str, io)?;
    let nodes = client.list_nodes(&channel_id)?;
    assert_eq!(nodes.len(), 1);

    let message1 = "This is a test message with words in it";
    let message2 = "Here we have a second message";
    client.send_message(&channel_id, message1)?;
    assert_eq!(client.message_count(&channel_id)?, 1);
    client.send_message(&channel_id, message2)?;
    assert_eq!(client.message_count(&channel_id)?, 2);

    let message = client.get_message(&channel_id, 2)?;
    assert_eq!(message.text, message2);
    let message = client.get_message(&channel_id, 1)?;
    assert_eq!(message.text, message1);

    Ok(())
}