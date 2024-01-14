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
    let mut data = [0; MEGA_BYTE];
    let io: MemIO<'_, SLAB_SIZE> = MemIO::new(&mut data)?;

    let mut client: Client<'_, MAX_CHANNELS, MAX_NODES, MemIO<'_, SLAB_SIZE>, RustCrypto> 
        = Client::new(key_pair, &mut crypto);

    let name_str = "Test Chat";
    let channel_id = client.init_chat(name_str, io)?;
    Ok(())
}