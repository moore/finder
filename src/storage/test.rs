use super::*;
use crate::storage::mem_io::MemIO;

#[test]
fn test_mem_io_new() -> Result<(), StorageError> {
    let mut data = [0; 128];
    let io: MemIO<'_, 64> = new_io(&mut data)?;
    assert_eq!(io.free_slabs()?, 2);
    Ok(())
}

#[test]
fn test_storage_new() -> Result<(), StorageError> {
    let mut data = [0; 4000];
    let io: MemIO<'_, 1000> = new_io(&mut data)?;

    let _storage = Storage::new(io);

    Ok(())
}

#[test]
fn test_storage_write_read() -> Result<(), StorageError> {
    let mut data = [0; 64];
    let io: MemIO<'_, 64> = new_io(&mut data)?;
    let mut storage = Storage::new(io);
    let mut writer = storage.get_writer()?;
    let mut data = [0; 1];

    for i in 0..3 {
        data[0] = i as u8;

        writer.write_record(i, 0, &data)?;
    }

    writer.commit();

    let mut cursor = storage
        .get_cursor_from_sequence(0)?
        .expect("expected to find cursor");
    let mut expect = 0;
    while let Some((data, next)) = storage.read(cursor)? {
        assert_eq!(data[0], expect);
        expect += 1;
        cursor = next;
    }
    assert_eq!(expect, 3);

    let mut cursor = storage
        .get_cursor_from_sequence(2)?
        .expect("expected to find cursor");

    let mut expect = 2;
    while let Some((data, next)) = storage.read(cursor)? {
        assert_eq!(data[0], expect);
        expect += 1;
        cursor = next;
    }
    assert_eq!(expect, 3);

    Ok(())
}

#[test]
fn test_storage_write_read2() -> Result<(), StorageError> {
    let mut data = [0; 128];
    let io: MemIO<'_, 64> = new_io(&mut data)?;
    let mut storage = Storage::new(io);
    let mut writer = storage.get_writer()?;
    let mut data = [0; 1];

    for i in 0..3 {
        data[0] = i as u8;
        writer.write_record(i, 0, &data)?;
    }

    writer.commit()?;

    writer = storage.get_writer()?;

    for i in 3..6 {
        data[0] = i as u8;

        writer.write_record(i, 0, &data)?;
    }

    writer.commit()?;

    let mut cursor = storage
        .get_cursor_from_sequence(0)?
        .expect("expected to find cursor");
    let mut expect = 0;
    while let Some((data, next)) = storage.read(cursor)? {
        assert_eq!(data[0], expect);
        expect += 1;
        cursor = next;
    }
    assert_eq!(expect, 6);

    let mut cursor = storage
        .get_cursor_from_sequence(3)?
        .expect("expected to find cursor");

    let mut expect = 3;
    while let Some((data, next)) = storage.read(cursor)? {
        assert_eq!(data[0], expect);
        expect += 1;
        cursor = next;
    }
    assert_eq!(expect, 6);

    Ok(())
}

fn new_io<'a, const DB_SIZE: usize, const SLAB_SIZE: usize>(
    data: &'a mut [u8; DB_SIZE],
) -> Result<MemIO<'a, SLAB_SIZE>, StorageError> {
    MemIO::new(data)
}
