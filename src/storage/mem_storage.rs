use super::*;

struct Record<T, const MAX_RECORD: usize> {
    data: Vec<u8, MAX_RECORD>,
    min_sequence: u64,
    _phantom: PhantomData<T>,
}


pub struct MemStorage<T, const DB_SIZE: usize, const MAX_RECORD: usize> {
    data: Vec<Record<T, MAX_RECORD>, DB_SIZE>,
}

impl<T, const DB_SIZE: usize, const MAX_RECORD: usize> MemStorage<T, DB_SIZE,  MAX_RECORD> {
    pub fn new() -> Self {
        MemStorage {
            data: Vec::new(),
        }
    }
}

impl<T, const DB_SIZE: usize, const MAX_RECORD: usize> Storage<T> for MemStorage<T, DB_SIZE,  MAX_RECORD> {

    fn get_cursor_from(&self, sequence: u64) -> Result<Option<Cursor>, StorageError> {
        let index = self.data.binary_search_by_key(&sequence, |r| r.min_sequence);
       
        let result = match index {
            Err(_) => None,
            Ok(mut i) => {
                // Walk backwards in case we did not
                // find the first instance
                while i > 0 {
                    let prev = i.saturating_sub(1);
                    let p_record = self.data.get(prev)
                        .ok_or(StorageError::Unreachable)?;
                    if p_record.min_sequence == sequence {
                        i = prev;
                    } else {
                        break;
                    }
                }

                Some(Cursor { chunk: 0, offset: i })
            },
        };

        Ok(result)
    }

    fn read<'a>(&'a self, cursor: &mut  Cursor) -> Option<&'a [u8]> {
        let record = self.data.get(cursor.offset);

        match record {
            None => None,
            Some(r) => {
                cursor.offset = cursor.offset.saturating_add(1);
                Some(r.data.as_slice())
            }
        }
    }

    fn add_record<'a>(&mut self, data: &'a [u8], min_sequence: u64) -> Result<(), StorageError> {

        let Ok(data) = Vec::from_slice(data) else {
            return Err(StorageError::RecordTooLarge(MAX_RECORD));
        };

        let record = Record {
            data: data,
            min_sequence,
            _phantom: PhantomData::<T>,
        };

        if let Err(_) = self.data.push(record) {
            return Err(StorageError::DbFull(DB_SIZE));
        }

        Ok(())
    }
}