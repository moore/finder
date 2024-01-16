
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
    static mut buffer: [u8 ; MEGA_BYTE] = [0u8; MEGA_BYTE];
    let data = unsafe { &mut buffer };
    let io: MemIO<'_, SLAB_SIZE> = MemIO::new(data)?;

    //let mut client: Client<'_, MAX_CHANNELS, MAX_NODES, MemIO<'_, SLAB_SIZE>, RustCrypto> 
    //    = Client::new(key_pair, &mut crypto);

    const channels_const: ClientChannels<MAX_CHANNELS, MAX_NODES, MemIO<'_, SLAB_SIZE>, RustCrypto> = ClientChannels::new();

    let channels = unsafe {
        &mut channels_const
    };

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
    static mut buffer: [u8 ; MEGA_BYTE] = [0u8; MEGA_BYTE];
    let data = unsafe { &mut buffer };

    let channel_id = {
    let io: MemIO<'_, SLAB_SIZE> = MemIO::new(data)?;

    const channels_const: ClientChannels<MAX_CHANNELS, MAX_NODES, MemIO<'_, SLAB_SIZE>, RustCrypto> = ClientChannels::new();

    let channels = unsafe {
        &mut channels_const
    };

    let mut client: Client<'_, '_, MAX_CHANNELS, MAX_NODES, MemIO<'_, SLAB_SIZE>, RustCrypto> 
        = Client::new(key_pair.clone(), &mut crypto, channels);

    let name_str = "Test Chat";
    client.init_chat(name_str, io)?
    };

    let io: MemIO<'_, SLAB_SIZE> = MemIO::new(data)?;

    const channels_const: ClientChannels<MAX_CHANNELS, MAX_NODES, MemIO<'_, SLAB_SIZE>, RustCrypto> = ClientChannels::new();

    let channels = unsafe {
        &mut channels_const
    };

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
    static mut buffer: [u8 ; MEGA_BYTE] = [0u8; MEGA_BYTE];
    let data = unsafe { &mut buffer };
    let io: MemIO<'_, SLAB_SIZE> = MemIO::new(data)?;

    const channels_const: ClientChannels<MAX_CHANNELS, MAX_NODES, MemIO<'_, SLAB_SIZE>, RustCrypto> = ClientChannels::new();
    
    let channels = unsafe {
        &mut channels_const
    };

    let mut client: Client<'_, '_, MAX_CHANNELS, MAX_NODES, MemIO<'_, SLAB_SIZE>, RustCrypto> 
        = Client::new(key_pair, &mut crypto, channels);

    let name_str = "Test Chat";
    let channel_id = client.init_chat(name_str, io)?;
    let nodes = client.list_nodes(&channel_id)?;
    assert_eq!(nodes.len(), 1);

    client.send_message(&channel_id, "This is a test message with words in it")?;
    assert_eq!(client.message_count(&channel_id)?, 1);
    client.send_message(&channel_id, "Here we have a second message")?;
    assert_eq!(client.message_count(&channel_id)?, 2);


    Ok(())
}