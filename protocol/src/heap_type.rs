use core::mem;
use core::cell::RefCell;
use critical_section::Mutex;
use critical_section;

#[derive(Debug)]
pub enum GuardCellError {
    AlreadyUsed,
    Unreachable,
}

pub struct GuardCell<T> {
    taken: bool,
    data: T,
}

pub struct StaticAllocation<T>(Mutex<RefCell<GuardCell<T>>>);

impl<T> StaticAllocation<T>
where
    T: 'static,
{
    pub const fn wrap(inner: T) -> StaticAllocation<T> {
        let cell = GuardCell {
            taken: false,
            data: inner,
        };
        StaticAllocation(Mutex::new(RefCell::new(cell)))
    }

    pub fn take_mut(&'static self) -> Result<&'static mut T, GuardCellError> {


        let result = critical_section::with(|cs| {
            let StaticAllocation(inner) = self;
            let mut cell = inner.borrow(cs).borrow_mut();

            if cell.taken {
                return Err(GuardCellError::AlreadyUsed);
            }

            // SAFETY: THis is safe because of the taken guard
            cell.taken = true;
            

            let result = unsafe {
                // SAFETY: This is safe because `T` is defined as `'static`
                // and the `taken` guard prevents taking more then once.
                mem::transmute::<&mut T, &'static mut T>(&mut cell.data)
            };

            Ok(result)
        })?; 

        Ok(result)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_safe_static() {
        static test: StaticAllocation<[u8; 1000]> = StaticAllocation::wrap([0u8; 1000]);
        let _mut_ref = test.take_mut().expect("should have gotten ref");

        test.take_mut()
            .expect_err("second takes should have failed");
    }
}
