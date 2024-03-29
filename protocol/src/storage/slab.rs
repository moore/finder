use super::*;


#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Slab<'a> {
    slab: usize,
    offset: usize,
    count: u32,
    slab_max_sequence: u64,
    // [count: u32][slab_max_sequence: u64][length:u32][data: [u8]]
    records: &'a [u8], // Record Data
}

impl<'a> Slab<'a> {
    pub fn new(data: &'a [u8], index: usize) -> Result<Self, StorageError> {
        let (count, offset) = read_u32(data, 0)?;
        let (slab_max_sequence, offset) = read_u64(data, offset)?;

        Ok(Slab {
            slab: index,
            offset,
            count,
            slab_max_sequence,
            records: data,
        })
    }

    pub fn record_count(&self) -> u32 {
        self.count
    }

    pub fn get_head(&self) -> Cursor {
        Cursor {
            slab: self.slab,
            offset: self.offset,
            read_count: 0,
        }
    }

    pub fn read(&self, mut cursor: Cursor) -> Result<Option<(Record<'a>, Cursor)>, StorageError> {
        if cursor.read_count >= self.record_count() {
            return Ok(None);
        }

        let at = cursor.offset;
        let (length, offset) = read_u32(self.records, at)?;

        // We don't really track the end of what is used
        // so if we read a zero length we assume we are
        // passed the end. This is kinda janky.
        if length == 0 {
            return Ok(None);
        }

        // BUG: what happens if usize is smaller then u32?
        let end_offset = offset
            .checked_add(length as usize)
            .ok_or(StorageError::CorruptDB)?;

        let Some(slice) = self.records.get(offset..end_offset) else {
            return Err(StorageError::CorruptDB);
        };

        let record: Record<'_> = from_bytes(slice)?;
        cursor.offset = end_offset;
        cursor.read_count = cursor
            .read_count
            .checked_add(1)
            .ok_or(StorageError::Unreachable)?;

        Ok(Some((record, cursor)))
    }
}

pub struct SlabWriter<'a, I: IO> {
    count: u32,
    slab_max_sequence: u64,
    slab_offset: usize,
    offset: usize,
    end: usize,
    // [count: u32][slab_max_sequence: u64][length:u32][max_sequence: u64][data: [u8]]
    io: &'a mut I, // Record Data
}

impl<'a, I: IO> SlabWriter<'a, I> {
    pub fn new(io: &'a mut I, offset: usize, end: usize) -> SlabWriter<'a, I> {
        const INITIAL_OFFSET: usize = size_of::<u32>() + size_of::<u64>();
        debug_assert!(INITIAL_OFFSET < (end - offset));
        Self {
            count: 0,
            slab_max_sequence: 0,
            slab_offset: offset,
            offset: offset + INITIAL_OFFSET,
            end,
            io,
        }
    }

    pub fn write_record(
        &mut self,
        max_sequence: u64,
        message_count: u64,
        sequence: u64,
        sender: NodeId,
        data: &[u8],
    ) -> Result<(), StorageError> {
        let record = Record {
            max_sequence,
            message_count,
            sequence,
            sender,
            data,
        };

        if self.slab_max_sequence > record.max_sequence {
            return Err(StorageError::OutOfOrder);
        }

        let end = self.io.write_record(self.offset, self.end, &record)?;
        // Update this first as it is the only
        // book keeping that can fail.
        self.count = self.count.checked_add(1).ok_or(StorageError::Unreachable)?;

        self.slab_max_sequence = record.max_sequence;

        self.offset = end;
        Ok(())
    }

    pub fn commit(self) -> Result<(), StorageError> {
        self.io
            .commit(self.count, self.slab_max_sequence, self.slab_offset)?;
        Ok(())
    }
}
