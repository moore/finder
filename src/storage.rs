use core::mem::size_of;

use super::*;
mod mem_io;
use mem_io::*;

mod slab;
use slab::*;

#[derive(Debug)]
pub enum StorageError {
    DbFull,
    RecordTooLarge(usize),
    Unreachable,
    CorruptDB,
    OutOfBounds,
    Unimplemented,
    PostcardError(postcard::Error),
    OutOfOrder,
}

impl From<postcard::Error> for StorageError {
    fn from(value: postcard::Error) -> Self {
        StorageError::PostcardError(value)
    }
}

#[derive(Debug)]
pub struct Cursor {
    slab: usize,
    offset: usize,
}

pub struct IoData<'a> {
    data: &'a mut [u8],
}
pub trait IO {
    fn truncate(&mut self) -> Result<(), StorageError>;
    fn slab_size(&self) -> usize;
    fn free_slabs(&self) -> Result<usize, StorageError>;
    fn slab_count(&self) -> Result<usize, StorageError>; 
    fn get_slab<'a>(&'a self, cursor: &Cursor) -> Result<Slab<'a>, StorageError>;
    fn new_writer<'a>(&'a mut self) -> Result<SlabWriter<'a, Self>, StorageError> where Self: Sized;
    fn write_record(&mut self, offset: usize, record: &Record) -> Result<usize, StorageError>;
    fn commit(&mut self, record_count: u32, max_sequence: u64, offset: usize) -> Result<(), StorageError>;
    fn get_head(&self) -> Result<Cursor, StorageError>;
}


#[derive(Debug, Clone)]
pub struct Record<'a> {
    max_sequence: u64,
    offset: usize,
    data: &'a [u8],
}


pub struct Storage<I> where I: IO {
    io: I,
}

impl<I> Storage<I> where I: IO {
    const LENGTH_LENGTH: usize = 4;
    const SEQUENCE_LENGTH: usize = 8;

    pub fn new<'a>(io: I) -> Self {
        Storage {
            io,
        }
    }

    fn read_record<'a>(&self, data: &'a [u8], offset: usize) -> Result<(Record<'a>, usize), StorageError> {
        let at = offset;
        let (length, offset) = read_u32(data, offset)?;
        let (max_sequence, offset) = read_u64(data, offset)?;

        // BUG: what happens if usize is smaller then u32?
        let end_offset = offset.checked_add(length as usize)
          .ok_or(StorageError::CorruptDB)?;


        let record_slice = data.get(offset..end_offset)
            .ok_or(StorageError::CorruptDB)?;

        let record = Record {
            max_sequence,
            offset: at,
            data: record_slice,
        };
    
        Ok((record, end_offset))
    }


}

impl<I> Storage<I> where I: IO {

    fn get_cursor_from(&self, sequence: u64) -> Result<Option<Cursor>, StorageError> {
        let mut cursor = self.io.get_head()?;
        dbg!(&cursor, sequence);
        // FIXME: This should be binary search over slabs
        // instead of a linear scan.
        loop {
            dbg!("getting slab");
            let slab = match self.io.get_slab(&cursor)  {
                Ok(slab) => slab,
                Err(e) => {
                    match e {
                        StorageError::OutOfBounds => {
                            return Ok(None);
                        }
                        _ => {
                            return Err(e);
                        }
                    }
                }
            };

            dbg!("got slab");
            let mut maby_record = slab.read(&mut cursor)?;
            dbg!(&maby_record);
            while let Some(record) = &maby_record {
                if record.max_sequence >= sequence {
                    cursor.offset = record.offset;
                    return Ok(Some(cursor));
                }
            }

            match cursor.slab.checked_add(1) {
                Some(v) => cursor.slab = v,
                None => return Ok(None),
            }
            dbg!(&cursor);
        }

    }

    fn read<'a>(&'a self, cursor: &mut  Cursor) -> Result<&'a [u8], StorageError> {
        let slab = self.io.get_slab(&cursor)?;

        let record = slab.read(cursor)?
            .ok_or(StorageError::OutOfBounds)?;
        
        Ok(&record.data)
    }

    fn write_slab(&mut self, records: &[Record]) -> Result<(), StorageError> {
        let mut writer: SlabWriter<'_, I> = self.io.new_writer()?;

        for record in records {
            writer.write_record(&record)?;
        }
        writer.commit();

        Ok(())
    }

    fn get_writer<'a>(&'a mut self) -> Result<SlabWriter<'a, I>, StorageError> {
        self.io.new_writer()
    }
}

fn read_u32(data: &[u8], offset: usize) -> Result<(u32,usize), StorageError> {
        
    let (len_arr, len_end) = read_arr(data, offset)?;
    let length = u32::from_be_bytes(len_arr);

    Ok((length, len_end))
}

fn write_u32(value: u32, target: &mut [u8], mut offset: usize) -> Result<usize, StorageError> {
        
    let data = value.to_be_bytes();
    offset = write_arr(data, target, offset)?;

    Ok(offset)
}

fn read_u64(data: &[u8], offset: usize) -> Result<(u64,usize), StorageError> {
    
    let (len_arr, len_end) = read_arr(data, offset)?;
    let length = u64::from_be_bytes(len_arr);

    Ok((length, len_end))
}

fn write_u64(value: u64, target: &mut [u8], mut offset: usize) -> Result<usize, StorageError> {
        
    let data = value.to_be_bytes();
    offset = write_arr(data, target, offset)?;

    Ok(offset)
}

fn read_arr<const SIZE: usize>(data: &[u8],  offset: usize)
    -> Result<([u8;SIZE], usize), StorageError> {
    let end = offset.checked_add(SIZE)
        .ok_or(StorageError::CorruptDB)?;

    let slice = data.get(offset..end)
        .ok_or(StorageError::OutOfBounds)?;

    let Ok(arr) = slice.try_into() else {
        return Err(StorageError::Unreachable);
    };
    Ok((arr, end))
}

fn write_arr<const SIZE: usize>(data: [u8 ; SIZE], target: &mut [u8],  offset: usize)
    -> Result<usize, StorageError> {
    let end = offset.checked_add(SIZE)
        .ok_or(StorageError::CorruptDB)?;

    let slice = target.get_mut(offset..end)
        .ok_or(StorageError::OutOfBounds)?;


    slice.copy_from_slice(data.as_slice());

    Ok(end)
}

#[cfg(test)]
mod test;