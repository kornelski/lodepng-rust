use crate::{CompressSettings, DecompressSettings, Error, Result, zero_vec};
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use std::io::{Read, Write};

pub(crate) fn new_decompressor(inp: &[u8]) -> Result<ZlibDecoder<&[u8]>, Error> {

    if inp.len() < 2 {
        return Err(Error::new(53));
    }
    /*read information from zlib header*/
    if (inp[0] as u32 * 256 + inp[1] as u32) % 31 != 0 {
        /*error: 256 * in[0] + in[1] must be a multiple of 31, the FCHECK value is supposed to be made that way*/
        return Err(Error::new(24));
    }
    let cm = inp[0] as u32 & 15;
    let cinfo = ((inp[0] as u32) >> 4) & 15;
    let fdict = ((inp[1] as u32) >> 5) & 1;
    if cm != 8 || cinfo > 7 {
        /*error: only compression method 8: inflate with sliding window of 32k is supported by the PNG spec*/
        return Err(Error::new(25));
    }
    if fdict != 0 {
        /*error: the specification of PNG says about the zlib stream:
          "The additional flags shall not specify a preset dictionary."*/
        return Err(Error::new(26));
    }

    Ok(ZlibDecoder::new_with_buf(inp, zero_vec(32 * 1024)?))
}

pub(crate) fn decompress_into_vec(inp: &[u8], out: &mut Vec<u8>) -> Result<(), Error> {
    let mut z = new_decompressor(inp)?;
    out.try_reserve((inp.len() * 3 / 2).max(16*1024))?;
    let mut actual_len = out.len();
    out.resize(out.capacity(), 0);
    loop {
        let read = z.read(&mut out[actual_len..])?;
        if read > 0 {
            actual_len += read;
            if out.capacity() < actual_len + 64 * 1024 {
                out.truncate(actual_len); // copy less
                out.try_reserve(64 * 1024)?;
                out.resize(out.capacity(), 0);
            }
        } else {
            break;
        }
    }
    out.truncate(actual_len);
    Ok(())
}

pub(crate) fn decompress(inp: &[u8], settings: &DecompressSettings) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new(); out.try_reserve(inp.len() * 3 / 2)?;
    if let Some(cb) = settings.custom_zlib {
        (cb)(inp, &mut out, settings)?;
    } else {
        decompress_into_vec(inp, &mut out)?;
    }
    Ok(out)
}

pub(crate) fn new_compressor<W: Write>(outv: W, settings: &CompressSettings) -> Result<ZlibEncoder<W>, Error> {
    let level = settings.level();
    let level = if level == 0 {
        Compression::none()
    } else {
        Compression::new(level.min(9).into())
    };
    Ok(ZlibEncoder::new(outv, level))
}

/* compress using the default or custom zlib function */
pub(crate) fn old_ffi_zlib_compress(inp: &[u8], settings: &CompressSettings) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new(); out.try_reserve(inp.len() / 2)?;
    compress_into(&mut out, inp, settings)?;
    Ok(out)
}

pub(crate) fn compress_into<W: Write>(out: &mut W, inp: &[u8], settings: &CompressSettings) -> Result<(), Error> {

    #[allow(deprecated)]
    if let Some(cb) = settings.custom_zlib {
        (cb)(inp, out, settings)?;
    } else {
        let mut z = new_compressor(out, settings)?;
        z.write_all(inp)?;
    }
    Ok(())
}


pub(crate) fn compress_fast(source: &[u8], destination: &mut Vec<u8>) {
    let mut zlib = ZlibEncoder::new(destination, Compression::new(1));
    let _ = zlib.write_all(source);
}
