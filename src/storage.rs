use core::mem::size_of;

use super::*;
mod mem_io;
use mem_io::*;


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
    fn new_writer<'a>(&'a mut self) -> Result<SlabWriter<'a>, StorageError>;
    fn get_head(&self) -> Result<Cursor, StorageError>;
}


#[derive(Debug, Clone)]
struct Record<'a> {
    max_sequence: u64,
    offset: usize,
    data: &'a [u8],
}


#[derive(Debug, Serialize, Deserialize, Clone)]
struct Slab<'a> {
    slab: usize,
    offset: usize,
    count: u32,
    slab_max_sequence: u64,
    // [count: u32][slab_max_sequence: u64][length:u32][max_sequence: u64][data: [u8]]
    records: &'a [u8], // Record Data
}

impl<'a> Slab<'a> {
    fn new(data: &'a [u8], cursor: &Cursor) -> Result<Self, StorageError> {
        let (count, mut offset) = read_u32(data, cursor.offset)?;
        let (slab_max_sequence, offset) = read_u64(data, offset)?;

        Ok(Slab {
            slab: cursor.slab,
            offset, 
            count, 
            slab_max_sequence, 
            records: data,
        })
    }

    pub fn get_head(&self) -> Cursor {
        Cursor {
            slab: self.slab,
            offset: self.offset,
        }
    }

    pub fn read(&self, cursor: &mut Cursor) -> Result<Option<Record<'a>>, StorageError> {

        let mut offset = cursor.offset;
        let (length, offset) = read_u32(self.records, offset)?;
        let (max_sequence, offset) = read_u64(self.records, offset)?;

        // Bug: What happens if length > usize?
        let end = offset.checked_add(length as usize)
            .ok_or(StorageError::CorruptDB)?;

        let Some(slice) = self.records.get(offset..end) else {
            return Ok(None);
        };

        let record = Record {
            max_sequence,
            offset: cursor.offset,
            data: slice,
        };

        cursor.offset = end;

        Ok(Some(record))
    }
}

struct SlabWriter<'a> {
    count: u32,
    slab_max_sequence: u64,
    offset: usize,
    // [count: u32][slab_max_sequence: u64][length:u32][max_sequence: u64][data: [u8]]
    target: &'a mut [u8], // Record Data  
}

impl<'a> SlabWriter<'a> {
    pub fn new(target:  &'a mut [u8]) -> SlabWriter<'a> {
        const INITIAL_OFFSET: usize = size_of::<u32>() + size_of::<u64>();
        Self {
            count: 0,
            slab_max_sequence: 0,
            offset: INITIAL_OFFSET,
            target,
        }
    }

    pub fn write_record(&mut self, max_sequence: u64, data: &[u8]) -> Result<(), StorageError> {
        if self.slab_max_sequence > max_sequence {
            return Err(StorageError::OutOfOrder);
        }
        
        let mut offset = self.offset;
        let end = offset.checked_add(data.len())
            .ok_or(StorageError::OutOfBounds)?;

        let slice = self.target.get_mut(offset..end)
            .ok_or(StorageError::OutOfBounds)?;
        
        slice.copy_from_slice(data);

        // Update this first as it is the only
        // book keeping that can fail.
        self.count = self.count.checked_add(1)
            .ok_or(StorageError::Unreachable)?;

        self.slab_max_sequence = max_sequence;

        self.offset = end;

        Ok(())
    }

    fn commit(self) -> Result<(), StorageError> {
        let offset = write_u32(self.count, self.target, 0)?;
        write_u64(self.slab_max_sequence, self.target, offset)?;
        Ok(())
    }
}

pub struct Storage<T, I> where I: IO {
    io: I,
    _phantom: PhantomData<T>,
}

impl<T, I> Storage<T, I> where I: IO {
    const LENGTH_LENGTH: usize = 4;
    const SEQUENCE_LENGTH: usize = 8;

    pub fn new<'a>(io: I) -> Self {
        Storage {
            io,
            _phantom: PhantomData::<T>
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

impl<T, I> Storage<T, I> where I: IO {

    fn get_cursor_from(&self, sequence: u64) -> Result<Option<Cursor>, StorageError> {
        let mut cursor = self.io.get_head()?;
        
        // FIXME: This should be binary search over slabs
        // instead of a linear scan.
        loop {
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

            let mut maby_record = slab.read(&mut cursor)?;

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
        }

    }

    fn read<'a>(&'a self, cursor: &mut  Cursor) -> Result<&'a [u8], StorageError> {
        let slab = self.io.get_slab(&cursor)?;

        let record = slab.read(cursor)?
            .ok_or(StorageError::OutOfBounds)?;
        
        Ok(&record.data)
    }

    fn write_slab(&mut self, records: &[Record]) -> Result<(), StorageError> {
        let mut writer = self.io.new_writer()?;

        for record in records {
            writer.write_record(record.max_sequence, record.data)?;
        }
        writer.commit();

        Ok(())
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

