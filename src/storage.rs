use core::mem::size_of;
use ascon_hash::{AsconXof, ExtendableOutput, Update, XofReader};

use super::*;
pub mod mem_io;
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

#[derive(Debug, Clone)]
pub struct Cursor {
    slab: usize,
    offset: usize,
}

pub trait IO {
    fn truncate(&mut self) -> Result<(), StorageError>;
    fn slab_size(&self) -> usize;
    fn free_slabs(&self) -> Result<usize, StorageError>;
    fn slab_count(&self) -> Result<usize, StorageError>;
    fn get_slab<'a>(&'a self, index: usize) -> Result<Slab<'a>, StorageError>;
    fn new_writer<'a>(&'a mut self) -> Result<SlabWriter<'a, Self>, StorageError>
    where
        Self: Sized;
    fn write_record(&mut self, offset: usize, record: &Record) -> Result<usize, StorageError>;
    fn commit(
        &mut self,
        record_count: u32,
        max_sequence: u64,
        offset: usize,
    ) -> Result<(), StorageError>;
    fn get_head(&self) -> Result<usize, StorageError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record<'a> {
    max_sequence: u64,
    message_count: u64,
    data: &'a [u8],
}

/* 
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbInfo {
    root1_offset: usize,
    root2_offset: usize,
    wal_offset: usize,
    wall_length: usize,
}


const CHECKSUM_SIZE: usize = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbRoot {
    generation: u64,
    data_start: usize,
    data_end: usize,
    check_sum: [u8 ; CHECKSUM_SIZE],
}

impl DbRoot {
    pub fn new(generation: u64, data_start: usize, data_end: usize) -> Self {
        let mut check_sum = 
            DbRoot::compute_checksum(generation, data_start, data_end);

        Self {
            generation,
            data_start,
            data_end,
            check_sum,
        }
    }

    pub fn validate(&self) -> Result<(), StorageError> {
        let computed = DbRoot::compute_checksum(
            self.generation, 
            self.data_start, 
            self.data_end);

        if computed != self.check_sum {
            return Err(StorageError::CorruptDB);
        }
        Ok(())
    }

    fn compute_checksum(generation: u64, data_start: usize, data_end: usize) -> [u8; CHECKSUM_SIZE] {
        let mut xof = AsconXof::default();
        xof.update(&generation.to_be_bytes());
        xof.update(&data_start.to_be_bytes());
        xof.update(&data_end.to_be_bytes());
        let mut reader = xof.finalize_xof();
        let mut check_sum = [0u8; CHECKSUM_SIZE];
        reader.read(&mut check_sum);
        check_sum
    }
}

/// Layout.
/// [DbInfo, Padded to whole page]
/// [Root 1, Padded to whole page]
/// [WAL, Padded to whole page]
/// [Root 2, Padded to whole page]
/// [Records]
*/

pub struct Storage<I>
where
    I: IO,
{
    io: I,
}

impl<I> Storage<I>
where
    I: IO,
{
    const LENGTH_LENGTH: usize = 4;
    const SEQUENCE_LENGTH: usize = 8;

    pub fn new<'a>(io: I) -> Self {
        Storage { io }
    }


    pub fn get_cursor_from(&self, sequence: u64) -> Result<Option<Cursor>, StorageError> {
        let mut index = self.io.get_head()?;

        loop {
            let slab = match self.io.get_slab(index) {
                Ok(slab) => slab,
                Err(e) => match e {
                    StorageError::OutOfBounds => {
                        return Ok(None);
                    }
                    _ => {
                        return Err(e);
                    }
                },
            };

            let mut cursor = slab.get_head();
            let mut curosr_copy = cursor.clone();
            while let Some((record, next)) = slab.read(cursor)? {
                if record.max_sequence >= sequence {
                    return Ok(Some(curosr_copy));
                }
                curosr_copy = next.clone();
                cursor = next;
            }

            match index.checked_add(1) {
                Some(v) => index = v,
                None => return Ok(None),
            }
        }
    }

    pub fn read<'a>(
        &'a self,
        mut cursor: Cursor,
    ) -> Result<Option<(&'a [u8], Cursor)>, StorageError> {
        let slab_index = cursor.slab;
        let slab = self.io.get_slab(slab_index)?;

        if let Some((record, next)) = slab.read(cursor)? {
            return Ok(Some((record.data, next)));
        }

        let Some(next_index) = slab_index.checked_add(1) else {
            return Ok(None);
        };

        if next_index >= self.io.slab_count()? {
            return Ok(None);
        }

        let slab = self.io.get_slab(next_index)?;

        cursor = slab.get_head();

        if let Some((record, next)) = slab.read(cursor)? {
            return Ok(Some((record.data, next)));
        }

        // This could only happen if there was an empty
        // slab which should not happen
        Err(StorageError::Unreachable)
    }

    pub fn get_writer<'a>(&'a mut self) -> Result<SlabWriter<'a, I>, StorageError> {
        self.io.new_writer()
    }
}

fn read_u32(data: &[u8], offset: usize) -> Result<(u32, usize), StorageError> {
    let (len_arr, len_end) = read_arr(data, offset)?;
    let length = u32::from_be_bytes(len_arr);

    Ok((length, len_end))
}

fn write_u32(value: u32, target: &mut [u8], mut offset: usize) -> Result<usize, StorageError> {
    let data = value.to_be_bytes();
    offset = write_arr(data, target, offset)?;

    Ok(offset)
}

fn read_u64(data: &[u8], offset: usize) -> Result<(u64, usize), StorageError> {
    let (len_arr, len_end) = read_arr(data, offset)?;
    let length = u64::from_be_bytes(len_arr);

    Ok((length, len_end))
}

fn write_u64(value: u64, target: &mut [u8], mut offset: usize) -> Result<usize, StorageError> {
    let data = value.to_be_bytes();
    offset = write_arr(data, target, offset)?;

    Ok(offset)
}

fn read_arr<const SIZE: usize>(
    data: &[u8],
    offset: usize,
) -> Result<([u8; SIZE], usize), StorageError> {
    let end = offset.checked_add(SIZE).ok_or(StorageError::CorruptDB)?;

    let slice = data.get(offset..end).ok_or(StorageError::OutOfBounds)?;

    let Ok(arr) = slice.try_into() else {
        return Err(StorageError::Unreachable);
    };
    Ok((arr, end))
}

fn write_arr<const SIZE: usize>(
    data: [u8; SIZE],
    target: &mut [u8],
    offset: usize,
) -> Result<usize, StorageError> {
    let end = offset.checked_add(SIZE).ok_or(StorageError::CorruptDB)?;

    let slice = target
        .get_mut(offset..end)
        .ok_or(StorageError::OutOfBounds)?;

    slice.copy_from_slice(data.as_slice());

    Ok(end)
}

#[cfg(test)]
mod test;
