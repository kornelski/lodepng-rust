use std::slice;
use std::ptr;
use ffi::{lodepng_free, lodepng_realloc, Error};

#[allow(non_camel_case_types)]
pub struct ucvector {
    data: *mut u8,
    size: usize,
    allocsize: usize,
}

impl ucvector {
    pub fn into_vec(self) -> Vec<u8> {
        self.slice().to_owned()
    }

    pub fn into_raw(mut self) -> (*mut u8, usize) {
        let ret = (self.data, self.size);
        self.data = ptr::null_mut();
        ret
    }

    pub fn reserve(&mut self, add_size: usize) {
        let n = self.size.checked_add(add_size).unwrap();
        self.reserve_abs(n).unwrap();
    }

    fn reserve_abs(&mut self, allocsize: usize) -> Result<(), Error> {
        if allocsize > self.allocsize {
            let newsize = if allocsize > self.allocsize * 2 {
                allocsize
            } else {
                allocsize * 3 / 2
            };
            let data = unsafe { lodepng_realloc(self.data as *mut _, newsize) };
            if data.is_null() {
                return Err(Error(83));
            }
            self.allocsize = newsize;
            self.data = data as *mut u8;
        }
        Ok(())
    }

    pub fn extend_from_slice(&mut self, data: &[u8]) -> Result<(), Error> {
        let start_offset = self.size;
        let new_length = start_offset.checked_add(data.len()).ok_or(Error(77))?;
        self.resize(new_length)?;
        self.slice_mut()[start_offset..start_offset + data.len()].clone_from_slice(data);
        Ok(())
    }

    pub fn resize(&mut self, size: usize) -> Result<(), Error> {
        self.reserve_abs(size)?;
        self.size = size;
        Ok(())
    }

    pub fn new() -> Self {
        Self {
            data: ptr::null_mut(),
            allocsize: 0,
            size: 0,
        }
    }

    pub unsafe fn from_raw(buffer: &mut *mut u8, size: usize) -> Self {
        let data = *buffer;
        *buffer = ptr::null_mut();
        Self {
            data,
            allocsize: size,
            size: size,
        }
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn last_mut(&mut self) -> &mut u8 {
        unsafe {
            assert!(self.size > 0);
            &mut *self.data.offset(self.size as isize - 1)
        }
    }

    pub fn slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.data, self.size) }
    }

    pub fn slice_mut(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.data, self.size) }
    }

    pub fn push(&mut self, c: u8) {
        let n = self.size + 1;
        self.resize(n).unwrap();
        *self.last_mut() = c;
    }
}

impl Drop for ucvector {
    fn drop(&mut self) {
        unsafe {
            lodepng_free(self.data as *mut _);
        }
    }
}
