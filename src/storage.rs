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

#[derive(Debug, Clone)]
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
    data: &'a [u8],
}

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

    pub fn read_record<'a>(
        &self,
        data: &'a [u8],
        offset: usize,
    ) -> Result<(Record<'a>, usize), StorageError> {
        let (length, offset) = read_u32(data, offset)?;

        // BUG: what happens if usize is smaller then u32?
        let end_offset = offset
            .checked_add(length as usize)
            .ok_or(StorageError::CorruptDB)?;

        let mut record: Record<'_> = from_bytes(&data[offset..end_offset])?;

        Ok((record, end_offset))
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
