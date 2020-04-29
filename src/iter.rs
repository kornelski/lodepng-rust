use std;
use std::ffi::CStr;
use std::marker::PhantomData;
use std::os::raw::c_char;
use super::ChunkRef;
use crate::Error;

pub struct TextKeysCStrIter<'a> {
    pub(crate) k: *mut *mut c_char,
    pub(crate) v: *mut *mut c_char,
    pub(crate) n: usize,
    pub(crate) _p: PhantomData<&'a CStr>,
}

impl<'a> Iterator for TextKeysCStrIter<'a> {
    type Item = (&'a CStr, &'a CStr);
    fn next(&mut self) -> Option<Self::Item> {
        if self.n > 0 {
            unsafe {
                debug_assert!(!(*self.k).is_null()); let k = CStr::from_ptr(*self.k);
                debug_assert!(!(*self.v).is_null()); let v = CStr::from_ptr(*self.v);
                self.n -= 1;
                self.k = self.k.offset(1);
                self.v = self.v.offset(1);
                Some((k, v))
            }
        } else {
            None
        }
    }
}

pub struct ITextKeysIter<'a> {
    pub(crate) k: *mut *mut c_char,
    pub(crate) l: *mut *mut c_char,
    pub(crate) t: *mut *mut c_char,
    pub(crate) s: *mut *mut c_char,
    pub(crate) n: usize,
    pub(crate) _p: PhantomData<&'a str>,
}

/// Invalid encoding truncates the strings
impl<'a> Iterator for ITextKeysIter<'a> {
    type Item = (&'a str, &'a str, &'a str, &'a str);
    fn next(&mut self) -> Option<Self::Item> {
        if self.n > 0 {
            unsafe {
                debug_assert!(!(*self.k).is_null()); let k = CStr::from_ptr(*self.k);
                debug_assert!(!(*self.l).is_null()); let l = CStr::from_ptr(*self.l);
                debug_assert!(!(*self.t).is_null()); let t = CStr::from_ptr(*self.t);
                debug_assert!(!(*self.s).is_null()); let s = CStr::from_ptr(*self.s);
                self.n -= 1;
                self.k = self.k.offset(1);
                self.l = self.l.offset(1);
                self.t = self.t.offset(1);
                self.s = self.s.offset(1);
                Some((
                    cstr_to_str(k),
                    cstr_to_str(l),
                    cstr_to_str(t),
                    cstr_to_str(s)))
            }
        } else {
            None
        }
    }
}

fn cstr_to_str(s: &CStr) -> &str {
    match s.to_str() {
        Ok(s) => s,
        Err(e) => {
            std::str::from_utf8(&s.to_bytes()[0..e.valid_up_to()]).unwrap()
        }
    }
}

pub struct ChunksIter<'a> {
    pub(crate) iter: ChunksIterFallible<'a>,
}

impl<'a> ChunksIter<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            iter: ChunksIterFallible {
                data
            }
        }
    }
}

impl<'a> Iterator for ChunksIter<'a> {
    type Item = ChunkRef<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().and_then(|item| item.ok())
    }
}

pub struct ChunksIterFallible<'a> {
    pub(crate) data: &'a [u8],
}

impl<'a> Iterator for ChunksIterFallible<'a> {
    type Item = Result<ChunkRef<'a>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.data.is_empty() {
            return None;
        }
        let ch = match ChunkRef::new(self.data) {
            Ok(ch) => ch,
            Err(e) => return Some(Err(e)),
        };
        self.data = &self.data[ch.len() + 12..];
        Some(Ok(ch))
    }
}
