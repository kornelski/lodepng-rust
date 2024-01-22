use crate::{CompressSettings, DecompressSettings, Error, Result};
use flate2::Compression;
use flate2::write::{ZlibEncoder, ZlibDecoder};
use std::io::Write;

fn check_zlib_stream(inp: &[u8]) -> Result<(), Error> {
    if inp.len() < 2 {
        return Err(Error::new(53));
    }
    /*read information from zlib header*/
    if (u32::from(inp[0]) * 256 + u32::from(inp[1])) % 31 != 0 {
        /*error: 256 * in[0] + in[1] must be a multiple of 31, the FCHECK value is supposed to be made that way*/
        return Err(Error::new(24));
    }
    let cm = u32::from(inp[0]) & 15;
    let cinfo = (u32::from(inp[0]) >> 4) & 15;
    let fdict = (u32::from(inp[1]) >> 5) & 1;
    if cm != 8 || cinfo > 7 {
        /*error: only compression method 8: inflate with sliding window of 32k is supported by the PNG spec*/
        return Err(Error::new(25));
    }
    if fdict != 0 {
        /*error: the specification of PNG says about the zlib stream:
          "The additional flags shall not specify a preset dictionary."*/
        return Err(Error::new(26));
    }

    Ok(())
}

pub(crate) enum Decoder<'settings> {
    Flate(ZlibDecoder<Vec<u8>>),
    Custom(&'settings DecompressSettings, Vec<u8>, Vec<u8>),
}

impl Decoder<'_> {
    pub fn push(&mut self, chunk: &[u8]) -> Result<(), Error> {
        match self {
            Self::Flate(dec) => {
                dec.write_all(chunk).map_err(|_| Error::new(23))?;
            },
            Self::Custom(_, buf, _) => {
                buf.extend_from_slice(chunk);
            },
        }
        Ok(())
    }

    pub fn finish(self) -> Result<Vec<u8>, Error> {
        match self {
            Self::Flate(dec) => {
                Ok(dec.finish().map_err(|_| Error::new(23))?)
            },
            Self::Custom(settings, buf, mut out) => {
                check_zlib_stream(&buf)?;

                out.try_reserve((buf.len() * 3 / 2).max(16*1024))?;
                let cb = settings.custom_zlib.ok_or(Error::new(87))?; // can't fail
                (cb)(&buf, &mut out, settings)?;
                Ok(out)
            }
        }
    }
}

pub(crate) fn new_decompressor(out: Vec<u8>, zlib_data_size: usize, settings: &DecompressSettings) -> Decoder<'_> {
    if settings.custom_zlib.is_some() {
        let mut buf = Vec::new();
        let _ = buf.try_reserve_exact(zlib_data_size);
        Decoder::Custom(settings, buf, out)
    } else {
        Decoder::Flate(ZlibDecoder::new(out))
    }
}

#[inline(never)]
pub(crate) fn decompress_into_vec(inp: &[u8]) -> Result<Vec<u8>, Error> {
    check_zlib_stream(inp)?;
    let mut out = Vec::new();
    out.try_reserve((inp.len() * 3 / 2).max(16*1024))?;
    let mut dec = ZlibDecoder::new(out);
    dec.write_all(inp).map_err(|_| Error::new(23))?;
    dec.finish().map_err(|_| Error::new(23))
}

pub(crate) fn decompress(inp: &[u8], settings: &DecompressSettings) -> Result<Vec<u8>, Error> {
    if let Some(cb) = settings.custom_zlib {
        let mut out = Vec::new(); out.try_reserve(inp.len() * 3 / 2)?;
        (cb)(inp, &mut out, settings)?;
        Ok(out)
    } else {
        decompress_into_vec(inp)
    }
}

pub(crate) fn new_compressor<W: Write>(outv: W, settings: &CompressSettings) -> ZlibEncoder<W> {
    let level = settings.level();
    let level = if level == 0 {
        Compression::none()
    } else {
        Compression::new(level.min(9).into())
    };
    ZlibEncoder::new(outv, level)
}

#[inline(never)]
pub(crate) fn compress_into(out: &mut dyn Write, inp: &[u8], settings: &CompressSettings) -> Result<(), Error> {

    #[allow(deprecated)]
    if let Some(cb) = settings.custom_zlib {
        (cb)(inp, out, settings)?;
    } else {
        let mut z = new_compressor(out, settings);
        z.write_all(inp)?;
    }
    Ok(())
}

pub struct Estimator {
    gz: flate2::Compress,
    tmp: Vec<u8>,
}

impl Estimator {
    pub fn new(len: usize) -> Self {
        #[cfg(not(any(feature = "cfzlib", feature = "ngzlib")))]
        let gz = flate2::Compress::new(Compression::fast(), false);
        #[cfg(any(feature = "cfzlib", feature = "ngzlib"))]
        let gz = flate2::Compress::new_with_window_bits(Compression::fast(), false,
                (len+270).max(1<<9).min(1<<14).next_power_of_two().trailing_zeros() as u8);
        Self {
            gz,
            tmp: vec![0; len+270],
        }
    }

    pub(crate) fn estimate_compressed_size(&mut self, mut source: &[u8], _dict: &[u8]) -> usize {
        self.gz.reset();
        #[cfg(any(feature = "cfzlib", feature = "ngzlib"))]
        if !_dict.is_empty() {
            let _ = self.gz.set_dictionary(&_dict[_dict.len().saturating_sub(1<<14)..]);
        }

        let init_total_out = self.gz.total_out();
        loop {
            let last_total_in = self.gz.total_in();
            let last_total_out = self.gz.total_out();
            let _ = self.gz.compress(source, &mut self.tmp, flate2::FlushCompress::Sync);
            let consumed = (self.gz.total_in() - last_total_in) as usize;
            let written = (self.gz.total_out() - last_total_out) as usize;
            if (consumed == 0 && written == 0) || consumed >= source.len() {
                break;
            }
            source = &source[consumed..];
        }
        (self.gz.total_out() - init_total_out) as _
    }
}

