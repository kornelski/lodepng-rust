use std::slice;
use std::ptr;
use ffi::{lodepng_malloc, Error};

#[allow(non_camel_case_types)]
pub struct ucvector {
    pub inner: Vec<u8>,
}

impl ucvector {
    pub fn into_vec(self) -> Vec<u8> {
        self.inner
    }

    pub fn from_vec(inner: Vec<u8>) -> Self {
        Self {
            inner
        }
    }

    pub fn into_raw(self) -> (*mut u8, usize) {
        unsafe {
            let len = self.inner.len();
            let data = lodepng_malloc(len) as *mut u8;
            if data.is_null() {
                return (ptr::null_mut(), 0)
            }
            let s = slice::from_raw_parts_mut(data, len);
            s.clone_from_slice(&self.inner);
            (data, len)
        }
    }

    #[inline(always)]
    pub fn resize(&mut self, size: usize, val: u8) {
        self.inner.resize(size, val)
    }

    pub fn reserve(&mut self, add_size: usize) {
        self.inner.reserve(add_size);
    }

    #[inline(always)]
    pub fn extend_from_slice(&mut self, data: &[u8]) -> Result<(), Error> {
        self.inner.extend_from_slice(data);
        Ok(())
    }

    pub fn new() -> Self {
        Self {
            inner: Vec::new()
        }
    }

    pub unsafe fn from_raw(buffer: &mut *mut u8, size: usize) -> Self {
        let s = slice::from_raw_parts(*buffer, size);
        *buffer = ptr::null_mut();
        Self {
            inner: s.to_owned(),
        }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[inline(always)]
    pub fn last_mut(&mut self) -> &mut u8 {
        let last = self.inner.len()-1;
        &mut self.inner[last]
    }

    #[inline(always)]
    pub fn slice(&self) -> &[u8] {
        &self.inner
    }

    #[inline(always)]
    pub fn slice_mut(&mut self) -> &mut [u8] {
        &mut self.inner
    }

    #[inline(always)]
    pub fn push(&mut self, c: u8) {
        self.inner.push(c)
    }
}
