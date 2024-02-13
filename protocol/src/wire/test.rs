use super::*;

extern crate alloc;
use alloc::vec::Vec;

#[test]
fn write_read() -> Result<(), WireError> {
    let mut message = [0u8 ; 1024];

    for i in 0..message.len() {
        message[i] = (i % 256) as u8;
    }

    let mtu = 250;
    let mut writer = WireWriter::new(1, mtu, &message, 3);

    let mut buffer = [0u8 ; 250];

    let len = writer.next(&mut buffer)?;

    let mut data = &buffer[0..len];
    let mut reader = WireReader::new(data, mtu)?;


    let mut result: Option<Vec<u8>> = None;

    for _ in 0..writer.packet_count() {
        result = reader.accept_packet(data)?;
        
        if result.is_some() {
            break;
        }

        let len = writer.next(&mut buffer)?;
        data = &buffer[0..len];
    }

    assert!(result.is_some());
    assert_eq!(result.unwrap().as_slice(), message.as_slice());

    Ok(())

}