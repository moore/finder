use super::*;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Slab<'a> {
    slab: usize,
    offset: usize,
    count: u32,
    slab_max_sequence: u64,
    // [count: u32][slab_max_sequence: u64][length:u32][max_sequence: u64][data: [u8]]
    records: &'a [u8], // Record Data
}

impl<'a> Slab<'a> {
    pub fn new(data: &'a [u8], cursor: &Cursor) -> Result<Self, StorageError> {
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
        let at = cursor.offset;
        let (length, offset) = read_u32(self.records, at)?;

        // BUG: what happens if usize is smaller then u32?
        let end_offset = offset.checked_add(length as usize)
          .ok_or(StorageError::CorruptDB)?;

        let Some(slice) = self.records.get(offset..end_offset) else {
            return Ok(None);
        };

        let mut record: Record<'_> = from_bytes(slice)?;

        record.offset = at;

        cursor.offset = end_offset;

        Ok(Some(record))
    }
}

pub struct SlabWriter<'a, I: IO> {
    count: u32,
    slab_max_sequence: u64,
    slab_offset: usize,
    offset: usize,
    // [count: u32][slab_max_sequence: u64][length:u32][max_sequence: u64][data: [u8]]
    io: &'a mut I, // Record Data  
}

impl<'a, I:IO> SlabWriter<'a, I> {
    pub fn new(io: &'a mut I, offset: usize) -> SlabWriter<'a, I> {
        const INITIAL_OFFSET: usize = size_of::<u32>() + size_of::<u64>();
        Self {
            count: 0,
            slab_max_sequence: 0,
            slab_offset: offset,
            offset: offset + INITIAL_OFFSET,
            io,
        }
    }

    pub fn write_record(&mut self, record: &Record) -> Result<(), StorageError> {

        if self.slab_max_sequence > record.max_sequence {
            return Err(StorageError::OutOfOrder);
        }
        
        let end = self.io.write_record(self.offset, record)?;

        // Update this first as it is the only
        // book keeping that can fail.
        self.count = self.count.checked_add(1)
            .ok_or(StorageError::Unreachable)?;

        self.slab_max_sequence = record.max_sequence;

        self.offset = end;
        Ok(())
    }

    pub fn commit(self) -> Result<(), StorageError> {
        self.io.commit(self.count, self.slab_max_sequence, self.slab_offset)?;
        Ok(())
    }
}
