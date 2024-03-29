use super::*;
pub struct MemIO<'a, const SLAB_SIZE: usize>
where
    Self: 'a,
{
    slab_count: usize,
    max_index: usize,
    start_offset: usize,
    data: &'a mut [u8],
}

impl<'a, const SLAB_SIZE: usize> MemIO<'a, SLAB_SIZE> {
    pub fn new(data: &'a mut [u8]) -> Result<Self, StorageError> {
        let max_index = data
            .len()
            .checked_div(SLAB_SIZE)
            .ok_or(StorageError::Unreachable)?;

        let result = Self {
            slab_count: 0,
            max_index,
            start_offset: 0,
            data,
        };
        Ok(result)
    }
}

impl<'a, const SLAB_SIZE: usize> IO for MemIO<'a, SLAB_SIZE> {
    fn truncate(&mut self) -> Result<(), StorageError> {
        Err(StorageError::Unimplemented)
    }

    fn slab_size(&self) -> usize {
        SLAB_SIZE
    }

    fn free_slabs(&self) -> Result<usize, StorageError> {
        Ok((self.data.len() - (self.slab_count * SLAB_SIZE)) / SLAB_SIZE)
    }

    fn slab_count(&self) -> Result<usize, StorageError> {
        Ok(self.slab_count)
    }

    fn new_writer<'b>(&'b mut self) -> Result<SlabWriter<'b, Self>, StorageError> {
        // BUG: used checked math
        let start = self.start_offset + ((self.slab_count) * SLAB_SIZE);
        let end = start + SLAB_SIZE;

        let writer = SlabWriter::new(self, start, end);

        Ok(writer)
    }

    fn get_slab<'b>(&'b self, index: usize) -> Result<Slab<'b>, StorageError> {
        if index >= self.max_index {
            return Err(StorageError::OutOfBounds);
        }

        // BUG: Used checked math
        let slab_start = (self.start_offset + (index * SLAB_SIZE)) % self.data.len();
        let slab_slice: &'b [u8] = &self.data[slab_start..(slab_start + SLAB_SIZE)];

        let records: Slab<'b> = Slab::new(slab_slice, index)?;
        Ok(records)
    }

    fn write_record(&mut self, offset: usize, end: usize, record: &Record) -> Result<usize, StorageError> {
        let len_offset = offset;
        // BUG: what if usize is u64
        let offset = len_offset
            .checked_add(size_of::<u32>())
            .ok_or(StorageError::OutOfBounds)?;

        let target = &mut self.data[offset..end];
        let wrote = to_slice(record, target)?;
        let wrote_len = wrote.len();
        write_u32(wrote_len as u32, self.data, len_offset)?;

        let end = offset
            .checked_add(wrote_len)
            .ok_or(StorageError::OutOfBounds)?;

        Ok(end)
    }

    fn commit(
        &mut self,
        record_count: u32,
        max_sequence: u64,
        offset: usize,
    ) -> Result<(), StorageError> {
        let offset = write_u32(record_count, self.data, offset)?;
        write_u64(max_sequence, self.data, offset)?;
        self.slab_count = self
            .slab_count
            .checked_add(1)
            .ok_or(StorageError::Unreachable)?;
        Ok(())
    }

    fn get_head(&self) -> Result<usize, StorageError> {
        Ok(self.start_offset)
    }
}
