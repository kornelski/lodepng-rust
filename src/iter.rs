use crate::ffi::LatinText;
use crate::ffi::IntlText;
use super::ChunkRef;
use crate::Error;

pub struct TextKeysIter<'a> {
    pub(crate) s: &'a [LatinText],
}

/// Item is: key value
impl<'a> Iterator for TextKeysIter<'a> {
    /// key value
    type Item = (&'a [u8], &'a [u8]);
    fn next(&mut self) -> Option<Self::Item> {
        if let Some((first, rest)) = self.s.split_first() {
            self.s = rest;
            Some((
                &first.key,
                &first.value,
            ))
        } else {
            None
        }
    }
}

pub struct ITextKeysIter<'a> {
    pub(crate) s: &'a [IntlText],
}

/// Item is: key langtag transkey value
impl<'a> Iterator for ITextKeysIter<'a> {
    /// key langtag transkey value
    type Item = (&'a str, &'a str, &'a str, &'a str);
    fn next(&mut self) -> Option<Self::Item> {
        if let Some((first, rest)) = self.s.split_first() {
            self.s = rest;
            Some((
                &first.key,
                &first.langtag,
                &first.transkey,
                &first.value,
            ))
        } else {
            None
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
