
use super::WireError;

extern crate alloc;
use alloc::vec::Vec;

use raptorq::{
    ObjectTransmissionInformation,
    Decoder,
    Encoder,
    EncodingPacket,
};

pub struct WireReader {
    pub message_number: u16,
    pub transfer_length: u16,
    decoder: Decoder,
}

// NOTE: We assume that there is a checksum and or error correction in layer 0
// so here we just have a label `0xa9f4` to check that is is a packet
// in the expected format.
// Packet format: [0xa9f4][2b block num][2b len][MTU - 6]
impl WireReader {
    pub fn new(data: &[u8], mtu: u16) -> Result<Self, WireError> {
        let (label , offset) = read_u16(data, 0)?;

        if label != 0xa9f4 {
            return Err(WireError::NotPacket);
        }

        let (message_number, offset) = read_u16(data, offset)?;
        let (transfer_length, _offset) = read_u16(data, offset)?;
        let max_packet_size = mtu - 6;
        let config = ObjectTransmissionInformation::with_defaults(transfer_length as u64, max_packet_size);
        let decoder = Decoder::new(config);

        Ok(Self{
            message_number,
            transfer_length,
            decoder,
        })
    }

    pub fn accept_packet(&mut self, data: &[u8]) -> Result<Option<Vec<u8>>, WireError> {
        let (label , offset) = read_u16(data, 0)?;

        if label != 0xa9f4 {
            return Err(WireError::NotPacket);
        }
        let (message_number, offset) = read_u16(data, offset)?;
        let (transfer_length, offset) = read_u16(data, offset)?;

        // We assume that if the either the `message_number` or `transfer_length`
        // don't match then this bust be a new block.
        if message_number != self.message_number || transfer_length != self.transfer_length {
            return Err(WireError::WrongBlock(message_number));
        }

        // BUG: can this panic?
        let data = &data[offset..];
        
        let packet = EncodingPacket::deserialize(data);

        let Some(received) = self.decoder.decode(packet) else {
            return Ok(None)
        };

        Ok(Some(received))
    }

    pub fn check_packet(data: &[u8]) -> Result<u16, WireError> {
        let (label , offset) = read_u16(data, 0)?;

        if label != 0xa9f4 {
            return Err(WireError::NotPacket);
        }

        let (message_number, _offset) = read_u16(data, offset)?;
        Ok(message_number)
    }
}

pub struct WireWriter {
    message_number: u16,
    transfer_length: u16,
    encoded: Vec<EncodingPacket>,
    last_sent: usize,
}

impl WireWriter {
    pub fn new(message_number: u16, mtu: u16, data: &[u8], repair_packets_per_block: u32) -> Self {

        if data.len() > u16::MAX as usize {
            // BUG: we should not panic
            panic!("data is too long");
        }

        let adjusted_mtu = mtu - 6; //[0xa9f4][u16]+[u16]
        let encoder = Encoder::with_defaults(data, adjusted_mtu);
        let encoded =  encoder.get_encoded_packets(repair_packets_per_block);
        Self {
            message_number, 
            transfer_length: data.len() as u16, 
            encoded,
            last_sent: 0
        }
    }

    pub fn packet_count(&self) -> usize {
        self.encoded.len()
    }

    pub fn next(&mut self, buffer: &mut [u8] ) -> Result<usize, WireError> {
        // SAFETY: Well we could wrap but and it would confuse
        // the have_more check but I don't think that is a big deal?
        self.last_sent = self.last_sent.wrapping_add(1);

        let index = self.last_sent % self.encoded.len();
        let offset = write_u16(0xa9f4, buffer, 0)?;
        let offset = write_u16(self.message_number, buffer, offset)?;
        let offset = write_u16(self.transfer_length, buffer, offset)?;

        let target = buffer.get_mut(offset..)
            .ok_or(WireError::OutOfBounds)?;

        let encoding = self.encoded.get(index)
            .ok_or(WireError::Unreachable)?;

        // I would like to just write it in to the target
        // but for some reason raptorq requires std to use
        // serde.
        let wrote = encoding.serialize();
        target[0..wrote.len()].copy_from_slice(&wrote);

        let wrote_length = 6 + wrote.len();

        Ok(wrote_length)
    }
}

fn read_u16(data: &[u8], offset: usize) -> Result<(u16, usize), WireError> {
    let (len_arr, len_end) = read_arr(data, offset)?;
    let length = u16::from_le_bytes(len_arr);

    Ok((length, len_end))
}

fn write_u16(value: u16, target: &mut [u8], mut offset: usize) -> Result<usize, WireError> {
    let data = value.to_le_bytes();
    offset = write_arr(data, target, offset)?;

    Ok(offset)
}

fn read_arr<const SIZE: usize>(
    data: &[u8],
    offset: usize,
) -> Result<([u8; SIZE], usize), WireError> {
    let end = offset.checked_add(SIZE).ok_or(WireError::Unreachable)?;

    let slice = data.get(offset..end).ok_or(WireError::OutOfBounds)?;

    let Ok(arr) = slice.try_into() else {
        return Err(WireError::Unreachable);
    };
    Ok((arr, end))
}

fn write_arr<const SIZE: usize>(
    data: [u8; SIZE],
    target: &mut [u8],
    offset: usize,
) -> Result<usize, WireError> {
    let end = offset.checked_add(SIZE).ok_or(WireError::Unreachable)?;

    let slice = target
        .get_mut(offset..end)
        .ok_or(WireError::OutOfBounds)?;

    slice.copy_from_slice(data.as_slice());

    Ok(end)
}