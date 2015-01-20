
use libc::c_void;
use libc::funcs::c95::stdlib::free;
use std::slice;
use std::mem::transmute;

/// Horrible kludge to free memory allocated by lodepng
pub struct CVec<T> {
    elements: usize,
    ptr: *mut T,
}

impl<T> CVec<T> {
    pub unsafe fn new(ptr: *mut T, elements: usize) -> CVec<T> {
        CVec {ptr:ptr, elements:elements}
    }

    /// Number of elements (pixels for pixel type, bytes for u8 type only)
    pub fn len(&self) -> usize {
        self.elements
    }

    /// *Copies* elements into a Vec
    pub fn to_vec(&self) -> Vec<T> {
        unsafe {
            Vec::from_raw_buf(self.ptr, self.elements)
        }
    }

    /// Exposes memory as slice without copying
    pub fn as_mut_slice<'a>(&'a mut self) -> &'a mut [T] {
        unsafe {
            slice::from_raw_mut_buf(&mut self.ptr, self.elements)
        }
    }

    /// Exposes memory as slice without copying
    pub fn as_slice<'a>(&'a self) -> &'a [T] {
        unsafe {
            slice::from_raw_buf(transmute(&self.ptr), self.elements)
        }
    }

    /// Exposes memory as slice without copying
    pub fn as_u8_slice<'a>(&'a self) -> &'a [u8] {
        unsafe {
            slice::from_raw_buf(transmute(&self.ptr), self.elements * ::std::mem::size_of::<T>())
        }
    }
}


#[unsafe_destructor]
impl<T> Drop for CVec<T> {
    fn drop(&mut self) {
        unsafe {
            free(self.ptr as *mut c_void);
        }
    }
}
