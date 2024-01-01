use super::*;
pub struct MemIO<'a, const SLAB_SIZE: usize, const DB_SIZE: usize>
    where Self: 'a {
    slab_count: usize,
    start_offset: usize,
    data: &'a mut [u8],
}

impl<'a, const SLAB_SIZE: usize, const DB_SIZE: usize> MemIO<'a, SLAB_SIZE, DB_SIZE> {
    pub fn new(data:&'a mut [u8]) -> Self {
        Self {
            slab_count: 0,
            start_offset: 0,
            data
        }
    }
}

impl<'a, const SLAB_SIZE: usize, const DB_SIZE: usize> IO 
    for MemIO<'a, SLAB_SIZE, DB_SIZE>  {

    fn truncate(&mut self) -> Result<(), StorageError> {
        Err(StorageError::Unimplemented)
    }

    fn slab_size(&self) -> usize {
        SLAB_SIZE
    }

    fn free_slabs(&self) -> Result<usize, StorageError> {
        Ok((DB_SIZE - (self.slab_count * SLAB_SIZE))/SLAB_SIZE)
    }

    fn slab_count(&self) -> Result<usize, StorageError> {
        Ok(self.slab_count)
    }

    fn new_writer<'b>(&'b mut self) -> Result<SlabWriter<'b>, StorageError> {
        // BUG: used checked math
        let start = self.start_offset + (( 1 + self.slab_count) * SLAB_SIZE);
        let end = start + SLAB_SIZE;
        let slice = self.data.get_mut(start..end)
            .ok_or(StorageError::DbFull)?;

        let writer = SlabWriter::new(slice);

        Ok(writer)
    }

    fn get_slab<'b>(&'b self, cursor: &Cursor) -> Result<Slab<'b>, StorageError> {
        if cursor.slab >= self.slab_count {
            return Err(StorageError::OutOfBounds)
        }

        // BUG: Used checked math
        let slab_start = (self.start_offset + (cursor.slab * SLAB_SIZE)) % DB_SIZE;
        let slab_slice: &'b [u8]  = &self.data[slab_start..(slab_start + SLAB_SIZE)];

        let records:Slab<'b>  = Slab::new(slab_slice, cursor)?;

        Ok(records)
    }

    fn get_head(&self) -> Result<Cursor, StorageError> {
        Ok(Cursor {
            slab: self.start_offset,
            offset: 0,
        })
    }
}