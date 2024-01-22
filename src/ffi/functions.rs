#![cfg_attr(feature = "cargo-clippy", allow(clippy::needless_borrow))]
#![allow(clippy::missing_safety_doc)]
#![allow(non_upper_case_globals)]

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod libc;

use crate::Error;
use super::{ColorMode, ColorProfile, ColorType, CompressSettings, DecoderSettings, DecompressSettings, EncoderSettings, ErrorCode, Info, State};
use crate::ChunkRef;
use crate::rustimpl::RGBA;
use crate::rustimpl;
use crate::zlib;
use std::path::Path;
use std::ffi::CStr;
use std::io::Read;
use std::io::Write;
use std::mem;
use std::os::raw::{c_char, c_long, c_uint, c_void};
use std::ptr;
use std::slice;

macro_rules! lode_error {
    ($e:expr) => {
        if let Err(e) = $e {
            ErrorCode::from(e)
        } else {
            ErrorCode(0)
        }
    };
}

macro_rules! lode_try {
    ($e:expr) => {{
        match $e {
            Err(e) => return ErrorCode::from(e),
            Ok(o) => o,
        }
    }};
}

macro_rules! lode_try_state {
    ($state:expr, $e:expr) => {{
        match $e {
            Err(err) => {
                $state = ErrorCode::from(err);
                return ErrorCode::from(err);
            },
            Ok(ok) => {
                $state = ErrorCode(0);
                ok
            }
        }
    }};
}

unsafe fn vec_from_raw(data: *mut u8, len: usize) -> Vec<u8> {
    if data.is_null() || 0 == len {
        return Vec::new();
    }
    slice::from_raw_parts_mut(data, len).to_owned()
}

fn vec_into_raw(v: Vec<u8>) -> Result<(*mut u8, usize), crate::Error> {
    unsafe {
        let len = v.len();
        let data = lodepng_malloc(len).cast::<u8>();
        if data.is_null() {
            Err(crate::Error::new(83))
        } else {
            slice::from_raw_parts_mut(data, len).clone_from_slice(&v);
            Ok((data, len))
        }
    }
}


#[no_mangle]
pub unsafe extern "C" fn lodepng_malloc(size: usize) -> *mut c_void {
    libc::malloc(size)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
    libc::realloc(ptr, size)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_free(ptr: *mut c_void) {
    libc::free(ptr);
}

#[no_mangle]
#[allow(deprecated)]
pub unsafe extern "C" fn lodepng_state_init(state: *mut State) {
    ptr::write(state, State::new());
}

#[no_mangle]
#[allow(deprecated)]
pub unsafe extern "C" fn lodepng_state_cleanup(state: &mut State) {
    // relies on the fact that empty State doesn't have any heap allocations
    *state = State::new();
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_state_copy(dest: *mut State, source: &State) -> ErrorCode {
    ptr::write(dest, source.clone());
    ErrorCode(0)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_error_text(code: ErrorCode) -> *const u8 {
    code.c_description().as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_encode32(out: &mut *mut u8, outsize: &mut usize, image: *const u8, w: c_uint, h: c_uint) -> ErrorCode {
    if image.is_null() {
        return ErrorCode(48);
    }
    try_vec_into_raw(out, outsize, rustimpl::lodepng_encode_memory(slice::from_raw_parts(image, 0x1FFF_FFFF), w, h, ColorType::RGBA, 8))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_encode24(out: &mut *mut u8, outsize: &mut usize, image: *const u8, w: c_uint, h: c_uint) -> ErrorCode {
    if image.is_null() {
        return ErrorCode(48);
    }
    try_vec_into_raw(out, outsize, rustimpl::lodepng_encode_memory(slice::from_raw_parts(image, 0x1FFF_FFFF), w, h, ColorType::RGB, 8))
}

#[inline]
fn load_file(filename: &Path) -> Result<Vec<u8>, Error> {
    std::fs::read(filename).map_err(|_| Error::new(78))
}

/*write given buffer to the file, overwriting the file, it doesn't append to it.*/
fn save_file(buffer: &[u8], filename: &Path) -> Result<(), Error> {
    std::fs::write(filename, buffer)
        .map_err(|_| Error::new(79))
}

#[inline]
fn encode_file(filename: &Path, image: &[u8], w: u32, h: u32, colortype: ColorType, bitdepth: u32) -> Result<(), Error> {
    let v = rustimpl::lodepng_encode_memory(image, w, h, colortype, bitdepth)?;
    save_file(&v, filename)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_encode_file(filename: *const c_char, image: *const u8, w: c_uint, h: c_uint, colortype: ColorType, bitdepth: c_uint) -> ErrorCode {
    if image.is_null() {
        return ErrorCode(48);
    }
    lode_error!(encode_file(&c_path(filename), slice::from_raw_parts(image, 0x1FFF_FFFF), w, h, colortype, bitdepth))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_encode32_file(filename: *const c_char, image: *const u8, w: c_uint, h: c_uint) -> ErrorCode {
    if image.is_null() {
        return ErrorCode(48);
    }
    lode_error!(encode_file(&c_path(filename), slice::from_raw_parts(image, 0x1FFF_FFFF), w, h, ColorType::RGBA, 8))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_encode24_file(filename: *const c_char, image: *const u8, w: c_uint, h: c_uint) -> ErrorCode {
    if image.is_null() {
        return ErrorCode(48);
    }
    lode_error!(encode_file(&c_path(filename), slice::from_raw_parts(image, 0x1FFF_FFFF), w, h, ColorType::RGB, 8))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_get_bpp_lct(colortype: ColorType, bitdepth: c_uint) -> c_uint {
    colortype.bpp(bitdepth) as _
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_get_bpp(info: &ColorMode) -> c_uint {
    info.bpp()
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_get_channels(info: &ColorMode) -> c_uint {
    c_uint::from(info.channels())
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_is_greyscale_type(info: &ColorMode) -> c_uint {
    c_uint::from(info.is_greyscale_type())
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_is_alpha_type(info: &ColorMode) -> c_uint {
    c_uint::from(info.is_alpha_type())
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_is_palette_type(info: &ColorMode) -> c_uint {
    c_uint::from(info.is_palette_type())
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_has_palette_alpha(info: &ColorMode) -> c_uint {
    c_uint::from(info.has_palette_alpha())
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_can_have_alpha(info: &ColorMode) -> c_uint {
    c_uint::from(info.can_have_alpha())
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_get_raw_size(w: c_uint, h: c_uint, color: &ColorMode) -> usize {
    color.raw_size(w, h)
}

fn get_raw_size_lct(w: u32, h: u32, colortype: ColorType, bitdepth: u32) -> usize {
    /*will not overflow for any color type if roughly w * h < 268435455*/
    let bpp = colortype.bpp(bitdepth) as usize;
    let n = w as usize * h as usize;
    ((n / 8) * bpp) + ((n & 7) * bpp + 7) / 8
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_get_raw_size_lct(w: c_uint, h: c_uint, colortype: ColorType, bitdepth: c_uint) -> usize {
    get_raw_size_lct(w, h, colortype, bitdepth)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_palette_clear(info: &mut ColorMode) {
    info.palette_clear();
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_palette_add(info: &mut ColorMode, r: u8, g: u8, b: u8, a: u8) -> ErrorCode {
    lode_error!(info.palette_add(RGBA { r, g, b, a }))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_clear_text(info: &mut Info) {
    info.clear_text();
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_add_text(info: &mut Info, key: *const c_char, str: *const c_char) -> ErrorCode {
    let k = lode_try!(CStr::from_ptr(key).to_str().map_err(|_| ErrorCode(89)));
    let s = lode_try!(CStr::from_ptr(str).to_str().map_err(|_| ErrorCode(89)));
    lode_error!(info.add_text(k, s))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_clear_itext(info: &mut Info) {
    info.clear_itext();
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_add_itext(info: &mut Info, key: *const c_char, langtag: *const c_char, transkey: *const c_char, str: *const c_char) -> ErrorCode {
    let k = CStr::from_ptr(key);
    let l = CStr::from_ptr(langtag);
    let t = CStr::from_ptr(transkey);
    let s = CStr::from_ptr(str);
    lode_error!(info.push_itext(k.to_bytes(), l.to_bytes(), t.to_bytes(), s.to_bytes()))
}

/// Terrible hack.
#[no_mangle]
pub unsafe extern "C" fn lodepng_chunk_create(out: &mut *mut u8, outsize: &mut usize, length: c_uint, type_: *const [u8; 4], data: *const u8) -> ErrorCode {
    let data = slice::from_raw_parts(data, length as usize);
    let type_ = type_.as_ref().unwrap();

    // detect it's our Vec, not malloced buf
    if *outsize == 0 && !(*out).is_null() {
        let vec = std::mem::transmute::<&mut *mut u8, &mut &mut Vec<u8>>(out);
        let mut ch = rustimpl::ChunkBuilder::new(vec, type_);
        lode_try!(ch.extend_from_slice(data));
        lode_error!(ch.finish())
    } else {
        let mut v = vec_from_raw(*out, *outsize);
        let mut ch = rustimpl::ChunkBuilder::new(&mut v, type_);
        lode_try!(ch.extend_from_slice(data));
        let err = lode_error!(ch.finish());
        let (data, size) = lode_try!(vec_into_raw(v));
        *out = data;
        *outsize = size;
        err
    }
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_chunk_length(chunk: *const u8) -> c_uint {
    rustimpl::chunk_length(slice::from_raw_parts(chunk, 12)) as c_uint
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_chunk_type(type_: &mut [u8; 5], chunk: *const u8) {
    let ch = ChunkRef::from_ptr(chunk).unwrap();
    let t = ch.name();
    type_[0..4].clone_from_slice(&t);
    type_[4] = 0;
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_chunk_type_equals(chunk: *const u8, name: &[u8; 4]) -> u8 {
    if name.iter().any(|&t| t == 0) {
        return 0;
    }
    let ch = ChunkRef::from_ptr(chunk).unwrap();
    u8::from(name == &ch.name())
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_chunk_data_const(chunk: *const u8) -> *const u8 {
    ChunkRef::from_ptr(chunk).unwrap().data().as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_chunk_next(chunk: *mut u8) -> *mut u8 {
    let len = lodepng_chunk_length(chunk) as usize;
    chunk.add(len + 12)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_chunk_next_const(chunk: *const u8) -> *const u8 {
    let len = lodepng_chunk_length(chunk) as usize;
    chunk.add(len + 12)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_chunk_ancillary(chunk: *const u8) -> u8 {
    u8::from(ChunkRef::from_ptr(chunk).unwrap().is_ancillary())
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_chunk_private(chunk: *const u8) -> u8 {
    u8::from(ChunkRef::from_ptr(chunk).unwrap().is_private())
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_chunk_safetocopy(chunk: *const u8) -> u8 {
    u8::from(ChunkRef::from_ptr(chunk).unwrap().is_safe_to_copy())
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_chunk_data(chunk_data: *mut u8) -> *mut u8 {
    chunk_data.add(8)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_chunk_check_crc(chunk: *const u8) -> c_uint {
    c_uint::from(ChunkRef::from_ptr(chunk).unwrap().check_crc())
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_chunk_generate_crc(chunk: *mut u8) {
    rustimpl::lodepng_chunk_generate_crc(slice::from_raw_parts_mut(chunk, 0x7FFF_FFFF));
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_chunk_append(out: &mut *mut u8, outsize: &mut usize, chunk: *const u8) -> ErrorCode {
    let mut v = vec_from_raw(*out, *outsize);
    lode_try!(chunk_append(&mut v, slice::from_raw_parts(chunk, 0x7FFF_FFFF)));
    let (data, size) = lode_try!(vec_into_raw(v));
    *out = data;
    *outsize = size;
    ErrorCode(0)
}
#[inline]
fn chunk_append(out: &mut Vec<u8>, chunk: &[u8]) -> Result<(), crate::Error> {
    let total_chunk_length = rustimpl::chunk_length(chunk) + 12;
    out.try_reserve(total_chunk_length)?;
    out.extend_from_slice(&chunk[0..total_chunk_length]);
    Ok(())
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_color_mode_init(info: *mut ColorMode) {
    ptr::write(info, ColorMode::new());
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_color_mode_cleanup(info: &mut ColorMode) {
    let mut hack = mem::zeroed();
    ptr::swap(&mut hack, info);
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_color_mode_equal(a: &ColorMode, b: &ColorMode) -> c_uint {
    c_uint::from(rustimpl::lodepng_color_mode_equal(a, b))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_color_mode_copy(dest: *mut ColorMode, source: &ColorMode) -> ErrorCode {
    ptr::write(dest, source.clone());
    ErrorCode(0)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_zlib_decompress(out: &mut *mut u8, outsize: &mut usize, inp: *const u8, insize: usize, settings: &DecompressSettings) -> ErrorCode {
    *out = ptr::null_mut();
    *outsize = 0;
    let inp = if !inp.is_null() {
        slice::from_raw_parts(inp, insize)
    } else if insize != 0 {
        return ErrorCode(48);
    } else {
        &[][..]
    };
    let vec = lode_try!(zlib::decompress(inp, settings));
    try_vec_into_raw(out, outsize, Ok(vec))
}

#[no_mangle]
pub unsafe extern "C" fn zlib_decompress(out: &mut *mut u8, outsize: &mut usize, inp: *const u8, insize: usize, settings: &DecompressSettings) -> ErrorCode {
    let inp = if !inp.is_null() {
        slice::from_raw_parts(inp, insize)
    } else if insize != 0 {
        return ErrorCode(48);
    } else {
        &[][..]
    };
    try_vec_into_raw(out, outsize, zlib::decompress(inp, settings))
}

/* compress using the default or custom zlib function */
#[no_mangle]
pub unsafe extern "C" fn lodepng_zlib_compress(out: &mut *mut u8, outsize: &mut usize, inp: *const u8, insize: usize, settings: &CompressSettings) -> ErrorCode {
    let inp = if !inp.is_null() {
        slice::from_raw_parts(inp, insize)
    } else if insize != 0 {
        return ErrorCode(48);
    } else {
        &[][..]
    };
    let mut v = vec_from_raw(*out, *outsize);
    let mut z = zlib::new_compressor(&mut v, settings);
    let err = lode_error!(z.write_all(inp).map_err(Error::from));
    drop(z);
    let (data, size) = lode_try!(vec_into_raw(v));
    *out = data;
    *outsize = size;
    err
}

#[no_mangle]
pub unsafe extern "C" fn zlib_compress(out: &mut *mut u8, outsize: &mut usize, inp: *const u8, insize: usize, settings: &CompressSettings) -> ErrorCode {
    let inp = if !inp.is_null() {
        slice::from_raw_parts(inp, insize)
    } else if insize != 0 {
        return ErrorCode(48);
    } else {
        &[][..]
    };
    let mut vec = Vec::new();
    let _ = vec.try_reserve(inp.len() / 2);
    let res = zlib::compress_into(&mut vec, inp, settings);
    try_vec_into_raw(out, outsize, res.map(move |()| vec))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_compress_settings_init(settings: *mut CompressSettings) {
    ptr::write(settings, CompressSettings::new());
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_decompress_settings_init(settings: *mut DecompressSettings) {
    ptr::write(settings, DecompressSettings::new());
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_crc32(data: *const u8, length: usize) -> c_uint {
    if data.is_null() {
        return 0;
    }
    crc32fast::hash(slice::from_raw_parts(data, length))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_info_init(info: *mut Info) {
    ptr::write(info, Info::new());
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_info_cleanup(info: &mut Info) {
    // relies on the fact that empty Info doesn't have any heap allocations
    *info = Info::new();
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_info_copy(dest: *mut Info, source: &Info) -> ErrorCode {
    ptr::write(dest, source.clone());
    ErrorCode(0)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_info_swap(a: &mut Info, b: &mut Info) {
    mem::swap(a, b);
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_convert(out: *mut u8, image: *const u8, mode_out: &ColorMode, mode_in: &ColorMode, w: c_uint, h: c_uint) -> ErrorCode {
    if image.is_null() {
        return ErrorCode(48);
    }
    lode_error!(rustimpl::lodepng_convert(slice::from_raw_parts_mut(out, 0x1FFF_FFFF), slice::from_raw_parts(image, 0x1FFF_FFFF), mode_out, mode_in, w, h))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_inspect(w_out: &mut c_uint, h_out: &mut c_uint, state: &mut State, inp: *const u8, insize: usize) -> ErrorCode {
    if inp.is_null() {
        return ErrorCode(48);
    }
    let (info, w, h) = lode_try_state!(state.error, rustimpl::lodepng_inspect(&state.decoder, slice::from_raw_parts(inp, insize), false));
    state.info_png = info;
    *w_out = w as c_uint;
    *h_out = h as c_uint;
    ErrorCode(0)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_decode(out: &mut *mut u8, w_out: &mut c_uint, h_out: &mut c_uint, state: &mut State, inp: *const u8, insize: usize) -> ErrorCode {
    if inp.is_null() || insize == 0 {
        return ErrorCode(48);
    }
    *out = ptr::null_mut();
    let (v, w, h) = lode_try_state!(state.error, rustimpl::lodepng_decode(state, slice::from_raw_parts(inp, insize)));
    *w_out = w;
    *h_out = h;
    let (data, _) = lode_try!(vec_into_raw(v));
    *out = data;
    ErrorCode(0)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_decode_memory(out: &mut *mut u8, w_out: &mut c_uint, h_out: &mut c_uint, inp: *const u8, insize: usize, colortype: ColorType, bitdepth: c_uint) -> ErrorCode {
    *out = ptr::null_mut();
    if inp.is_null() || insize == 0 {
        return ErrorCode(48);
    }
    let (v, w, h) = lode_try!(rustimpl::lodepng_decode_memory(slice::from_raw_parts(inp, insize), colortype, bitdepth));
    *w_out = w;
    *h_out = h;
    let (data, _) = lode_try!(vec_into_raw(v));
    *out = data;
    ErrorCode(0)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_decode32(out: &mut *mut u8, w: &mut c_uint, h: &mut c_uint, inp: *const u8, insize: usize) -> ErrorCode {
    lodepng_decode_memory(out, w, h, inp, insize, ColorType::RGBA, 8)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_decode24(out: &mut *mut u8, w: &mut c_uint, h: &mut c_uint, inp: *const u8, insize: usize) -> ErrorCode {
    lodepng_decode_memory(out, w, h, inp, insize, ColorType::RGB, 8)
}

#[inline]
fn decode_file(filename: &Path, colortype: ColorType, bitdepth: u32) -> Result<(Vec<u8>, u32, u32), Error> {
    let buf = load_file(filename)?;
    rustimpl::lodepng_decode_memory(&buf, colortype, bitdepth)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_decode_file(out: &mut *mut u8, w_out: &mut c_uint, h_out: &mut c_uint, filename: *const c_char, colortype: ColorType, bitdepth: c_uint) -> ErrorCode {
    *out = ptr::null_mut();
    let (v, w, h) = lode_try!(decode_file(&c_path(filename), colortype, bitdepth));
    *w_out = w;
    *h_out = h;
    let (data, _) = lode_try!(vec_into_raw(v));
    *out = data;
    ErrorCode(0)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_decode32_file(out: &mut *mut u8, w: &mut c_uint, h: &mut c_uint, filename: *const c_char) -> ErrorCode {
    lodepng_decode_file(out, w, h, filename, ColorType::RGBA, 8)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_decode24_file(out: &mut *mut u8, w: &mut c_uint, h: &mut c_uint, filename: *const c_char) -> ErrorCode {
    lodepng_decode_file(out, w, h, filename, ColorType::RGB, 8)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_decoder_settings_init(settings: *mut DecoderSettings) {
    ptr::write(settings, DecoderSettings::new());
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_buffer_file(out: *mut u8, size: usize, filename: *const c_char) -> ErrorCode {
    let out = slice::from_raw_parts_mut(out, size);
    let res = std::fs::File::open(c_path(filename))
        .and_then(|mut f| f.read_exact(out))
        .map_err(|_| crate::Error::new(78));
    lode_error!(res)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_load_file(out: &mut *mut u8, outsize: &mut usize, filename: *const c_char) -> ErrorCode {
    try_vec_into_raw(out, outsize, load_file(&c_path(filename)))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_save_file(buffer: *const u8, buffersize: usize, filename: *const c_char) -> ErrorCode {
    if buffer.is_null() {
        return ErrorCode(48);
    }
    lode_error!(save_file(slice::from_raw_parts(buffer, buffersize), &c_path(filename)))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_encode(out: &mut *mut u8, outsize: &mut usize, image: *const u8, w: c_uint, h: c_uint, state: &mut State) -> ErrorCode {
    *out = ptr::null_mut();
    *outsize = 0;
    if image.is_null() {
        return ErrorCode(48);
    }
    let res = lode_try_state!(state.error, rustimpl::lodepng_encode(slice::from_raw_parts(image, 0x1FFF_FFFF), w as _, h as _, state));
    let (data, size) = lode_try!(vec_into_raw(res));
    *out = data;
    *outsize = size;
    ErrorCode(0)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_get_color_profile(profile_out: *mut ColorProfile, image: *const u8, w: c_uint, h: c_uint, mode: &ColorMode) -> ErrorCode {
    if image.is_null() {
        return ErrorCode(48);
    }
    let prof = rustimpl::get_color_profile(slice::from_raw_parts(image, 0x1FFF_FFFF), w, h, mode);
    ptr::write(profile_out, prof);
    ErrorCode(0)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_auto_choose_color(mode_out: &mut ColorMode, image: *const u8, w: c_uint, h: c_uint, mode_in: &ColorMode) -> ErrorCode {
    if image.is_null() {
        return ErrorCode(48);
    }
    let mode = lode_try!(rustimpl::auto_choose_color(slice::from_raw_parts(image, 0x1FFF_FFFF), w as _, h as _, mode_in));
    ptr::write(mode_out, mode);
    ErrorCode(0)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_filesize(filename: *const c_char) -> c_long {
    std::fs::metadata(c_path(filename)).map(|m| m.len()).ok()
        .map_or(-1, |l| l as c_long)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_encode_memory(out: &mut *mut u8, outsize: &mut usize, image: *const u8, w: c_uint, h: c_uint, colortype: ColorType, bitdepth: c_uint) -> ErrorCode {
    if image.is_null() {
        return ErrorCode(48);
    }
    try_vec_into_raw(out, outsize, rustimpl::lodepng_encode_memory(slice::from_raw_parts(image, 0x1FFF_FFFF), w, h, colortype, bitdepth))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_encoder_settings_init(settings: *mut EncoderSettings) {
    ptr::write(settings, EncoderSettings::new());
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_color_profile_init(prof: *mut ColorProfile) {
    ptr::write(prof, ColorProfile::new());
}

#[no_mangle]
#[allow(deprecated)]
pub static lodepng_default_compress_settings: CompressSettings = CompressSettings {
    btype: 0,
    use_lz77: true,
    windowsize: 0,
    minmatch: 0,
    nicematch: 0,
    lazymatching: false,
    custom_zlib: None,
    custom_deflate: None,
    custom_context: 0usize as *mut _,
};

#[no_mangle]
pub static lodepng_default_decompress_settings: DecompressSettings = DecompressSettings {
    custom_zlib: None,
    custom_inflate: None,
    custom_context: 0usize as *mut _,
};

#[cfg(unix)]
unsafe fn c_path<'meh>(filename: *const c_char) -> &'meh std::path::Path {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    assert!(!filename.is_null());
    let tmp = CStr::from_ptr(filename);
    std::path::Path::new(OsStr::from_bytes(tmp.to_bytes()))
}

#[cfg(not(unix))]
unsafe fn c_path(filename: *const c_char) -> std::path::PathBuf {
    let tmp = CStr::from_ptr(filename);
    tmp.to_string_lossy().to_string().into()
}

#[inline]
unsafe fn try_vec_into_raw(out: &mut *mut u8, outsize: &mut usize, result: Result<Vec<u8>, crate::Error>) -> ErrorCode {
    match result.and_then(vec_into_raw) {
        Ok((data, len)) => {
            *out = data;
            *outsize = len;
            ErrorCode(0)
        },
        Err(err) => {
            *out = ptr::null_mut();
            *outsize = 0;
            ErrorCode::from(err)
        },
    }
}
