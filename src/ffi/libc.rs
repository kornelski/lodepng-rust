pub type size_t = usize;

const MALLOC_HEADER: isize = 8;
const MALLOC_ALIGN: usize = 8;

use super::c_void;
use std::alloc::{self, Layout};
use std::ptr;

pub unsafe fn malloc(size: size_t) -> *mut c_void {
    let lay = Layout::from_size_align_unchecked(MALLOC_HEADER as usize + size, MALLOC_ALIGN);
    let p = alloc::alloc(lay);
    if p.is_null() {
        return ptr::null_mut();
    }
    *(p as *mut size_t) = size;
    p.offset(MALLOC_HEADER) as *mut c_void
}

pub unsafe fn free(p: *mut c_void) {
    let p = (p as *mut u8).offset(-MALLOC_HEADER);
    let size = *(p as *mut size_t);
    let lay = Layout::from_size_align_unchecked(MALLOC_HEADER as usize + size, MALLOC_ALIGN);
    alloc::dealloc(p, lay);
}

pub unsafe fn realloc(p: *mut c_void, new_size: size_t) -> *mut c_void {
    let p = (p as *mut u8).offset(-MALLOC_HEADER);
    let size = *(p as *mut size_t);
    let lay = Layout::from_size_align_unchecked(MALLOC_HEADER as usize + size, MALLOC_ALIGN);
    let p = alloc::realloc(p, lay, new_size);
    if p.is_null() {
        return ptr::null_mut();
    }
    *(p as *mut size_t) = new_size;
    p.offset(MALLOC_HEADER) as *mut c_void
}
