
use parking_lot::Mutex;
use core::mem;
use once_cell::unsync::OnceCell;

#[derive(Debug)]
pub enum SafeStaticError {
    AlreadyUsed,
    Unreachable,
}


pub struct GuardCell<T>(Mutex<OnceCell<T>>);

impl<T> GuardCell<T> where T: 'static{
    pub const fn wrap(inner: T) ->  GuardCell<T> {
        let cell = OnceCell::with_value(inner);
        GuardCell::<T>(Mutex::new(cell))
    }

    pub fn take_mut(&'static self) -> Result<&'static mut T, SafeStaticError> {
        let GuardCell(inner) = self;
        let mut guard = inner.try_lock()
            .ok_or(SafeStaticError::AlreadyUsed)?;

        let src = guard.get_mut()
            .ok_or(SafeStaticError::Unreachable)?;

        let result = unsafe {
            mem::transmute::<
                &mut T, 
                &'static mut T
                >(src)
        };

        mem::forget(guard);

        Ok(result)
    }
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_safe_static() {
        static test: GuardCell<[u8 ; 1000]> = GuardCell::wrap([0u8 ; 1000]);
        let mut_ref = test.take_mut()
            .expect("should have gotten ref");
        
        test.take_mut().expect_err("second takes should have failed");
    }
}



