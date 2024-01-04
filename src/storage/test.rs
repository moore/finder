
use super::*;

#[test]
fn test_mem_io_new() -> Result<(), StorageError> {

    let mut data = [0; 128];
    let io: MemIO<'_, 64> = new_io(&mut data);
    assert_eq!(io.free_slabs()?, 2);
    Ok(())
}

#[test]
fn test_storage_new() -> Result<(), StorageError> {

    let mut data = [0 ; 4000];
    let io: MemIO<'_, 1000> = new_io(&mut data);

    let storage = Storage::new(io);

    Ok(())
}


#[test]
fn test_storage_write_read() -> Result<(), StorageError> {
    let mut data = [0 ; 64];
    let io: MemIO<'_, 64> = new_io(&mut data);
    let mut storage = Storage::new(io);
    let mut writer = storage.get_writer()?;
    let mut data = [0;1];   

    for i in 0..3 {
        data[0] = i as u8;
        let record = Record {
            offset: 0,
            max_sequence: i,
            data: &data,
        };
        writer.write_record(&record)?;
    }

    writer.commit();

    let mut cursor = storage.get_cursor_from(0)?
        .expect("expected to find cursor");
    let mut expect = 0;
    while let Ok(data) = storage.read(&mut cursor) {
        assert_eq!(data[0], expect);
        expect += 1;
    }
    assert_eq!(expect, 3);  

    
    
    let mut cursor = storage.get_cursor_from(2)?
    .expect("expected to find cursor");

    let mut expect = 2;
    while let Ok(data) = storage.read(&mut cursor) {
        assert_eq!(data[0], expect);
        expect += 1;
    }
    assert_eq!(expect, 3);  
    
    Ok(())
}

fn new_io<'a, const DB_SIZE: usize, const SLAB_SIZE: usize>(data: &'a mut [u8 ; DB_SIZE]) -> MemIO<'a, SLAB_SIZE> {
    MemIO::new(data)
}