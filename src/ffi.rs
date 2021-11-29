#![cfg_attr(feature = "cargo-clippy", allow(needless_borrow))]
#![allow(non_upper_case_globals)]
use crate::ChunkRef;
use crate::rustimpl::RGBA;
use std::ptr;
use std::mem;
use std::slice;
use std::fmt;
use std::os::raw::{c_char, c_uint, c_long, c_void};
use std::ffi::CStr;
use std::path::*;
use std::io;

use crate::rustimpl;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod libc;

macro_rules! lode_error {
    ($e:expr) => {
        if let Err(e) = $e {
            ErrorCode::from(e)
        } else {
            ErrorCode(0)
        }
    }
}

macro_rules! lode_try {
    ($e:expr) => {{
        match $e {
            Err(e) => return ErrorCode::from(e),
            Ok(o) => o,
        }
    }}
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
    }}
}


unsafe fn vec_from_raw(data: *mut u8, len: usize) -> Vec<u8> {
    if data.is_null() || 0 == len {
        return Vec::new()
    }
    slice::from_raw_parts_mut(data, len).to_owned()
}

fn vec_into_raw(v: Vec<u8>) -> Result<(*mut u8, usize), crate::Error> {
    unsafe {
        let len = v.len();
        let data = lodepng_malloc(len) as *mut u8;
        if data.is_null() {
            Err(crate::Error::new(83))
        } else {
            slice::from_raw_parts_mut(data, len).clone_from_slice(&v);
            Ok((data, len))
        }
    }
}


#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct ErrorCode(pub c_uint);

/// Type for `decode`, `encode`, etc. Same as standard PNG color types.
#[repr(C)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ColorType {
    /// greyscale: 1, 2, 4, 8, 16 bit
    GREY = 0,
    /// RGB: 8, 16 bit
    RGB = 2,
    /// palette: 1, 2, 4, 8 bit
    PALETTE = 3,
    /// greyscale with alpha: 8, 16 bit
    GREY_ALPHA = 4,
    /// RGB with alpha: 8, 16 bit
    RGBA = 6,

    /// Not PNG standard, for internal use only. BGRA with alpha, 8 bit
    BGRA = 6|64,
    /// Not PNG standard, for internal use only. BGR no alpha, 8 bit
    BGR = 2|64,
    /// Not PNG standard, for internal use only. BGR no alpha, padded, 8 bit
    BGRX = 3|64,
}

/// Color mode of an image. Contains all information required to decode the pixel
/// bits to RGBA colors. This information is the same as used in the PNG file
/// format, and is used both for PNG and raw image data in LodePNG.
#[repr(C)]
#[derive(Clone)]
pub struct ColorMode {
    /// color type, see PNG standard
    pub colortype: ColorType,
    /// bits per sample, see PNG standard
    pub(crate) bitdepth: c_uint,

    /// palette (`PLTE` and `tRNS`)
    /// Dynamically allocated with the colors of the palette, including alpha.
    /// When encoding a PNG, to store your colors in the palette of the ColorMode, first use
    /// lodepng_palette_clear, then for each color use lodepng_palette_add.
    /// If you encode an image without alpha with palette, don't forget to put value 255 in each A byte of the palette.
    ///
    /// When decoding, by default you can ignore this palette, since LodePNG already
    /// fills the palette colors in the pixels of the raw RGBA output.
    ///
    /// The palette is only supported for color type 3.
    pub(crate) palette: Option<Box<[RGBA; 256]>>,
    /// palette size in number of colors (amount of bytes is 4 * `palettesize`)
    pub(crate) palettesize: usize,

    /// transparent color key (`tRNS`)
    ///
    /// This color uses the same bit depth as the bitdepth value in this struct, which can be 1-bit to 16-bit.
    /// For greyscale PNGs, r, g and b will all 3 be set to the same.
    ///
    /// When decoding, by default you can ignore this information, since LodePNG sets
    /// pixels with this key to transparent already in the raw RGBA output.
    ///
    /// The color key is only supported for color types 0 and 2.
    pub(crate) key_defined: c_uint,
    pub(crate) key_r: c_uint,
    pub(crate) key_g: c_uint,
    pub(crate) key_b: c_uint,
}

impl fmt::Debug for ColorMode {
    #[cold]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut s = f.debug_struct("ColorMode");
        s.field("colortype", &self.colortype);
        s.field("bitdepth", &self.bitdepth);
        s.field("palette", &self.palette.as_ref().map(|s| &s[..]));
        s.field("palettesize", &self.palettesize);
        s.field("key_defined", &self.key_defined);
        s.field("key_r", &self.key_r);
        s.field("key_g", &self.key_g);
        s.field("key_b", &self.key_b);
        s.finish()
    }
}

pub type custom_compress_callback =   Option<fn(input: &[u8], output: &mut dyn io::Write, context: &CompressSettings) -> Result<(), crate::Error>>;
pub type custom_decompress_callback = Option<fn(input: &[u8], output: &mut dyn io::Write, context: &DecompressSettings) -> Result<(), crate::Error>>;

#[repr(C)]
#[derive(Clone)]
pub struct DecompressSettings {
    pub(crate) custom_zlib: custom_decompress_callback,
    pub(crate) custom_inflate: custom_decompress_callback,
    pub(crate) custom_context: *const c_void,
}

impl fmt::Debug for DecompressSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("DecompressSettings");
        s.field("custom_zlib", &self.custom_zlib.is_some());
        s.field("custom_inflate", &self.custom_inflate.is_some());
        s.field("custom_context", &self.custom_context);
        s.finish()
    }
}

/// Settings for zlib compression. Tweaking these settings tweaks the balance between speed and compression ratio.
#[repr(C)]
#[derive(Clone)]
pub struct CompressSettings {
    /// Obsolete. No-op.
    #[deprecated]
    pub windowsize: u32,
    /// Compression level 1 (fast) to 9 (best). Use `set_level()` instead.
    #[deprecated]
    pub minmatch: u16,
    /// Obsolete. No-op.
    #[deprecated]
    pub nicematch: u16,
    /// Obsolete. No-op.
    #[deprecated]
    pub btype: u8,
    /// If false, it won't compress at all. Use `set_level(0)`
    #[deprecated]
    pub use_lz77: bool,
    /// Obsolete. No-op.
    #[deprecated]
    pub lazymatching: bool,
    /// use custom zlib encoder instead of built in one (default: None)
    pub custom_zlib: custom_compress_callback,
    /// use custom deflate encoder instead of built in one (default: null)
    /// if custom_zlib is used, custom_deflate is ignored since only the built in
    /// zlib function will call custom_deflate
    pub custom_deflate: custom_compress_callback,
    /// optional custom settings for custom functions
    pub custom_context: *const c_void,
}

impl CompressSettings {
    /// 0 (none), 1 (fast) to 9 (best)
    #[allow(deprecated)]
    pub fn set_level(&mut self, level: u8) {
        self.use_lz77 = level != 0;
        self.minmatch = level.into();
    }

    /// zlib compression level
    #[allow(deprecated)]
    pub fn level(&self) -> u8 {
        if self.use_lz77 {
            if self.minmatch > 0 && self.minmatch <= 9 {
            self.minmatch as _
            } else {
                7
            }
        } else {
            0
        }
    }
}

/// The information of a `Time` chunk in PNG
#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct Time {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

/// Information about the PNG image, except pixels, width and height
#[repr(C)]
#[derive(Debug, Clone)]
pub struct Info {
    /// interlace method of the original file
    pub interlace_method: u8,
    /// color type and bits, palette and transparency of the PNG file
    pub color: ColorMode,

    ///  suggested background color chunk (bKGD)
    ///  This color uses the same color mode as the PNG (except alpha channel), which can be 1-bit to 16-bit.
    ///
    ///  For greyscale PNGs, r, g and b will all 3 be set to the same. When encoding
    ///  the encoder writes the red one. For palette PNGs: When decoding, the RGB value
    ///  will be stored, not a palette index. But when encoding, specify the index of
    ///  the palette in background_r, the other two are then ignored.
    ///
    ///  The decoder does not use this background color to edit the color of pixels.
    pub background_defined: bool,
    /// red component of suggested background color
    pub background_r: u16,
    /// green component of suggested background color
    pub background_g: u16,
    /// blue component of suggested background color
    pub background_b: u16,

    /// set to 1 to make the encoder generate a tIME chunk
    pub time_defined: bool,
    /// time chunk (tIME)
    pub time: Time,

    /// if 0, there is no pHYs chunk and the values below are undefined, if 1 else there is one
    pub phys_defined: bool,
    /// pixels per unit in x direction
    pub phys_x: c_uint,
    /// pixels per unit in y direction
    pub phys_y: c_uint,
    /// may be 0 (unknown unit) or 1 (metre)
    pub phys_unit: u8,

    /// There are 3 buffers, one for each position in the PNG where unknown chunks can appear
    /// each buffer contains all unknown chunks for that position consecutively
    /// The 3 buffers are the unknown chunks between certain critical chunks:
    /// 0: IHDR-`PLTE`, 1: `PLTE`-IDAT, 2: IDAT-IEND
    /// Must be boxed for FFI hack.
    pub(crate) unknown_chunks: [Box<Vec<u8>>; 3],
    /// Ugly hack for back-compat with C FFI
    pub(crate) always_zero_for_ffi_hack: [usize; 3],

    ///  non-international text chunks (tEXt and zTXt)
    ///
    ///  The `char**` arrays each contain num strings. The actual messages are in
    ///  text_strings, while text_keys are keywords that give a short description what
    ///  the actual text represents, e.g. Title, Author, Description, or anything else.
    ///
    ///  A keyword is minimum 1 character and maximum 79 characters long. It's
    ///  discouraged to use a single line length longer than 79 characters for texts.
    pub(crate) texts: Vec<LatinText>,

    ///  international text chunks (iTXt)
    ///  Similar to the non-international text chunks, but with additional strings
    ///  "langtags" and "transkeys".
    pub(crate) itexts: Vec<IntlText>,
}

#[derive(Debug, Clone)]
pub(crate) struct LatinText {
    pub(crate) key: Box<[u8]>,
    pub(crate) value: Box<[u8]>,
}

#[derive(Debug, Clone)]
pub(crate) struct IntlText {
    pub(crate) key: Box<str>,
    pub(crate) langtag: Box<str>,
    pub(crate) transkey: Box<str>,
    pub(crate) value: Box<str>,
}

/// Settings for the decoder. This contains settings for the PNG and the Zlib decoder, but not the `Info` settings from the `Info` structs.
#[repr(C)]
#[derive(Clone, Debug)]
pub struct DecoderSettings {
    /// in here is the setting to ignore Adler32 checksums
    pub zlibsettings: DecompressSettings,
    /// ignore CRC checksums
    pub ignore_crc: bool,
    pub color_convert: bool,
    pub read_text_chunks: bool,
    pub remember_unknown_chunks: bool,
}

/// automatically use color type with less bits per pixel if losslessly possible. Default: `AUTO`
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FilterStrategy {
    /// every filter at zero
    ZERO = 0,
    /// Use filter that gives minumum sum, as described in the official PNG filter heuristic.
    MINSUM,
    /// Use the filter type that gives smallest Shannon entropy for this scanline. Depending
    /// on the image, this is better or worse than minsum.
    ENTROPY,
    /// Brute-force-search PNG filters by compressing each filter for each scanline.
    /// Experimental, very slow, and only rarely gives better compression than MINSUM.
    BRUTE_FORCE,
    /// use predefined_filters buffer: you specify the filter type for each scanline
    PREDEFINED,
}

#[repr(C)]
#[derive(Clone, Debug)]
pub struct EncoderSettings {
    /// settings for the zlib encoder, such as window size, ...
    pub zlibsettings: CompressSettings,
    /// how to automatically choose output PNG color type, if at all
    pub auto_convert: bool,
    /// If true, follows the official PNG heuristic: if the PNG uses a palette or lower than
    /// 8 bit depth, set all filters to zero. Otherwise use the filter_strategy. Note that to
    /// completely follow the official PNG heuristic, filter_palette_zero must be true and
    /// filter_strategy must be FilterStrategy::MINSUM
    pub filter_palette_zero: bool,
    /// Which filter strategy to use when not using zeroes due to filter_palette_zero.
    /// Set filter_palette_zero to 0 to ensure always using your chosen strategy. Default: FilterStrategy::MINSUM
    pub filter_strategy: FilterStrategy,

    /// used if filter_strategy is FilterStrategy::PREDEFINED. In that case, this must point to a buffer with
    /// the same length as the amount of scanlines in the image, and each value must <= 5. You
    /// have to cleanup this buffer, LodePNG will never free it. Don't forget that filter_palette_zero
    /// must be set to 0 to ensure this is also used on palette or low bitdepth images
    pub(crate) predefined_filters: *const u8,

    /// force creating a `PLTE` chunk if colortype is 2 or 6 (= a suggested palette).
    /// If colortype is 3, `PLTE` is _always_ created
    pub force_palette: bool,
    /// add LodePNG identifier and version as a text chunk, for debugging
    pub add_id: bool,
    /// encode text chunks as zTXt chunks instead of tEXt chunks, and use compression in iTXt chunks
    pub text_compression: bool,
}

/// The settings, state and information for extended encoding and decoding
#[repr(C)]
#[derive(Clone, Debug)]
pub struct State {
    pub decoder: DecoderSettings,
    pub encoder: EncoderSettings,

    /// specifies the format in which you would like to get the raw pixel buffer
    pub info_raw: ColorMode,
    /// info of the PNG image obtained after decoding
    pub info_png: Info,
    pub error: ErrorCode,
}

/// Gives characteristics about the colors of the image, which helps decide which color model to use for encoding.
/// Used internally by default if `auto_convert` is enabled. Public because it's useful for custom algorithms.
#[repr(C)]
pub struct ColorProfile {
    /// not greyscale
    pub colored: bool,
    /// image is not opaque and color key is possible instead of full alpha
    pub key: bool,
    /// key values, always as 16-bit, in 8-bit case the byte is duplicated, e.g. 65535 means 255
    pub key_r: u16,
    pub key_g: u16,
    pub key_b: u16,
    /// image is not opaque and alpha channel or alpha palette required
    pub alpha: bool,
    /// amount of colors, up to 257. Not valid if bits == 16.
    /// bits per channel (not for palette). 1,2 or 4 for greyscale only. 16 if 16-bit per channel required.
    pub bits: u8,
    pub numcolors: u16,
    /// Remembers up to the first 256 RGBA colors, in no particular order
    pub palette: [crate::RGBA; 256],
}

impl fmt::Debug for ColorProfile {
    #[cold]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ColorProfile")
            .field("colored", &self.colored)
            .field("key", &self.key)
            .field("key_r", &self.key_r)
            .field("key_g", &self.key_g)
            .field("key_b", &self.key_b)
            .field("alpha", &self.alpha)
            .field("numcolors", &self.numcolors)
            .field("bits", &self.bits)
            .finish()
    }
}

impl fmt::Debug for CompressSettings {
    #[allow(deprecated)]
    #[cold]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("CompressSettings");
        s.field("minmatch", &self.minmatch);
        s.field("use_lz77", &self.use_lz77);
        s.field("custom_zlib", &self.custom_zlib.is_some());
        s.field("custom_deflate", &self.custom_deflate.is_some());
        s.field("custom_context", &self.custom_context);
        s.finish()
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
    libc::free(ptr)
}

#[no_mangle]
#[allow(deprecated)]
pub unsafe extern "C" fn lodepng_state_init(state: *mut State) {
    ptr::write(state, State::new())
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
    assert!(!image.is_null());
    to_vec(out, outsize, rustimpl::lodepng_encode_memory(slice::from_raw_parts(image, 0x1FFF_FFFF), w, h, ColorType::RGBA, 8))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_encode24(out: &mut *mut u8, outsize: &mut usize, image: *const u8, w: c_uint, h: c_uint) -> ErrorCode {
    assert!(!image.is_null());
    to_vec(out, outsize, rustimpl::lodepng_encode_memory(slice::from_raw_parts(image, 0x1FFF_FFFF), w, h, ColorType::RGB, 8))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_encode_file(filename: *const c_char, image: *const u8, w: c_uint, h: c_uint, colortype: ColorType, bitdepth: c_uint) -> ErrorCode {
    assert!(!image.is_null());
    lode_error!(rustimpl::lodepng_encode_file(&c_path(filename), slice::from_raw_parts(image, 0x1FFF_FFFF), w, h, colortype, bitdepth))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_encode32_file(filename: *const c_char, image: *const u8, w: c_uint, h: c_uint) -> ErrorCode {
    assert!(!image.is_null());
    lode_error!(rustimpl::lodepng_encode_file(&c_path(filename), slice::from_raw_parts(image, 0x1FFF_FFFF), w, h, ColorType::RGBA, 8))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_encode24_file(filename: *const c_char, image: *const u8, w: c_uint, h: c_uint) -> ErrorCode {
    assert!(!image.is_null());
    lode_error!(rustimpl::lodepng_encode_file(&c_path(filename), slice::from_raw_parts(image, 0x1FFF_FFFF), w, h, ColorType::RGB, 8))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_get_bpp_lct(colortype: ColorType, bitdepth: c_uint) -> c_uint {
    rustimpl::lodepng_get_bpp_lct(colortype, bitdepth)
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
    info.is_greyscale_type() as c_uint
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_is_alpha_type(info: &ColorMode) -> c_uint {
    info.is_alpha_type() as c_uint
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_is_palette_type(info: &ColorMode) -> c_uint {
    info.is_palette_type() as c_uint
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_has_palette_alpha(info: &ColorMode) -> c_uint {
    info.has_palette_alpha() as c_uint
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_can_have_alpha(info: &ColorMode) -> c_uint {
    info.can_have_alpha() as c_uint
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_get_raw_size(w: c_uint, h: c_uint, color: &ColorMode) -> usize {
    color.raw_size(w, h)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_get_raw_size_lct(w: c_uint, h: c_uint, colortype: ColorType, bitdepth: c_uint) -> usize {
    rustimpl::lodepng_get_raw_size_lct(w, h, colortype, bitdepth)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_palette_clear(info: &mut ColorMode) {
    info.palette_clear()
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_palette_add(info: &mut ColorMode, r: u8, g: u8, b: u8, a: u8) -> ErrorCode {
    lode_error!(info.palette_add(RGBA{r, g, b, a}))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_clear_text(info: &mut Info) {
    info.clear_text()
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
        lode_error!(rustimpl::add_chunk(vec, type_, data))
    } else {
        let mut v = vec_from_raw(*out, *outsize);
        let err = lode_error!(rustimpl::add_chunk(&mut v, type_, data));
        let (data, size) = lode_try!(vec_into_raw(v));
        *out = data;
        *outsize = size;
        err
    }
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_chunk_length(chunk: *const u8) -> c_uint {
    rustimpl::lodepng_chunk_length(slice::from_raw_parts(chunk, 12)) as c_uint
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
    (name == &ch.name()) as u8
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
    ChunkRef::from_ptr(chunk).unwrap().is_ancillary() as u8
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_chunk_private(chunk: *const u8) -> u8 {
    ChunkRef::from_ptr(chunk).unwrap().is_private() as u8
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_chunk_safetocopy(chunk: *const u8) -> u8 {
    ChunkRef::from_ptr(chunk).unwrap().is_safe_to_copy() as u8
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_chunk_data(chunk_data: *mut u8) -> *mut u8 {
    chunk_data.add(8)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_chunk_check_crc(chunk: *const u8) -> c_uint {
    ChunkRef::from_ptr(chunk).unwrap().check_crc() as c_uint
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_chunk_generate_crc(chunk: *mut u8) {
    rustimpl::lodepng_chunk_generate_crc(slice::from_raw_parts_mut(chunk, 0x7FFF_FFFF))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_chunk_append(out: &mut *mut u8, outsize: &mut usize, chunk: *const u8) -> ErrorCode {
    let mut v = vec_from_raw(*out, *outsize);
    lode_try!(rustimpl::chunk_append(&mut v, slice::from_raw_parts(chunk, 0x7FFF_FFFF)));
    let (data, size) = lode_try!(vec_into_raw(v));
    *out = data;
    *outsize = size;
    ErrorCode(0)
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
    rustimpl::lodepng_color_mode_equal(a, b) as c_uint
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_color_mode_copy(dest: *mut ColorMode, source: &ColorMode) -> ErrorCode {
    ptr::write(dest, source.clone());
    ErrorCode(0)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_zlib_decompress(out: &mut *mut u8, outsize: &mut usize, inp: *const u8, insize: usize, _: &DecompressSettings) -> ErrorCode {
    to_vec(out, outsize, rustimpl::lodepng_zlib_decompress(slice::from_raw_parts(inp, insize)))
}

#[no_mangle]
pub unsafe extern "C" fn zlib_decompress(out: &mut *mut u8, outsize: &mut usize, inp: *const u8, insize: usize, settings: &DecompressSettings) -> ErrorCode {
    to_vec(out, outsize, rustimpl::zlib_decompress(slice::from_raw_parts(inp, insize), settings))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_zlib_compress(out: &mut *mut u8, outsize: &mut usize, inp: *const u8, insize: usize, settings: &CompressSettings) -> ErrorCode {
    let mut v = vec_from_raw(*out, *outsize);
    let err = lode_error!(rustimpl::lodepng_zlib_compress(&mut v, slice::from_raw_parts(inp, insize), settings));
    let (data, size) = lode_try!(vec_into_raw(v));
    *out = data;
    *outsize = size;
    err
}

#[no_mangle]
pub unsafe extern "C" fn zlib_compress(out: &mut *mut u8, outsize: &mut usize, inp: *const u8, insize: usize, settings: &CompressSettings) -> ErrorCode {
    to_vec(out, outsize, rustimpl::zlib_compress(slice::from_raw_parts(inp, insize), settings))
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
    rustimpl::lodepng_crc32(slice::from_raw_parts(data, length))
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
    mem::swap(a, b)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_convert(out: *mut u8, image: *const u8, mode_out: &ColorMode, mode_in: &ColorMode, w: c_uint, h: c_uint) -> ErrorCode {
    assert!(!image.is_null());
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
    *w_out = w as u32;
    *h_out = h as u32;
    let (data, _) = lode_try!(vec_into_raw(v));
    *out = data;
    ErrorCode(0)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_decode_memory(out: &mut *mut u8, w_out: &mut c_uint, h_out: &mut c_uint, inp: *const u8, insize: usize, colortype: ColorType, bitdepth: c_uint) -> ErrorCode {
    if inp.is_null() || insize == 0 {
        return ErrorCode(48);
    }
    *out = ptr::null_mut();
    let (v, w, h) = lode_try!(rustimpl::lodepng_decode_memory(slice::from_raw_parts(inp, insize), colortype, bitdepth));
    *w_out = w as u32;
    *h_out = h as u32;
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

#[no_mangle]
pub unsafe extern "C" fn lodepng_decode_file(out: &mut *mut u8, w_out: &mut c_uint, h_out: &mut c_uint, filename: *const c_char, colortype: ColorType, bitdepth: c_uint) -> ErrorCode {
    *out = ptr::null_mut();
    let (v, w, h) = lode_try!(rustimpl::lodepng_decode_file(&c_path(filename), colortype, bitdepth));
    *w_out = w as u32;
    *h_out = h as u32;
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
    lode_error!(rustimpl::lodepng_buffer_file(slice::from_raw_parts_mut(out, size), &c_path(filename)))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_load_file(out: &mut *mut u8, outsize: &mut usize, filename: *const c_char) -> ErrorCode {
    to_vec(out, outsize, rustimpl::lodepng_load_file(&c_path(filename)))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_save_file(buffer: *const u8, buffersize: usize, filename: *const c_char) -> ErrorCode {
    lode_error!(rustimpl::lodepng_save_file(slice::from_raw_parts(buffer, buffersize), &c_path(filename)))
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_encode(out: &mut *mut u8, outsize: &mut usize, image: *const u8, w: c_uint, h: c_uint, state: &mut State) -> ErrorCode {
    *out = ptr::null_mut();
    *outsize = 0;
    assert!(!image.is_null());
    let res = lode_try_state!(state.error, rustimpl::lodepng_encode(slice::from_raw_parts(image, 0x1FFF_FFFF), w, h, state));
    let (data, size) = lode_try!(vec_into_raw(res));
    *out = data;
    *outsize = size;
    ErrorCode(0)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_get_color_profile(profile_out: *mut ColorProfile, image: *const u8, w: c_uint, h: c_uint, mode: &ColorMode) -> ErrorCode {
    assert!(!image.is_null());
    let prof = lode_try!(rustimpl::get_color_profile(slice::from_raw_parts(image, 0x1FFF_FFFF), w, h, mode));
    ptr::write(profile_out, prof);
    ErrorCode(0)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_auto_choose_color(mode_out: &mut ColorMode, image: *const u8, w: c_uint, h: c_uint, mode_in: &ColorMode) -> ErrorCode {
    assert!(!image.is_null());
    let mode = lode_try!(rustimpl::auto_choose_color(slice::from_raw_parts(image, 0x1FFF_FFFF), w as usize, h as usize, mode_in));
    ptr::write(mode_out, mode);
    ErrorCode(0)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_filesize(filename: *const c_char) -> c_long {
    rustimpl::lodepng_filesize(&c_path(filename))
        .map_or(-1, |l| l as c_long)
}

#[no_mangle]
pub unsafe extern "C" fn lodepng_encode_memory(out: &mut *mut u8, outsize: &mut usize, image: *const u8, w: c_uint, h: c_uint, colortype: ColorType, bitdepth: c_uint) -> ErrorCode {
    assert!(!image.is_null());
    to_vec(out, outsize, rustimpl::lodepng_encode_memory(slice::from_raw_parts(image, 0x1FFF_FFFF), w, h, colortype, bitdepth))
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
unsafe fn c_path<'meh>(filename: *const c_char) -> &'meh Path {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    assert!(!filename.is_null());
    let tmp = CStr::from_ptr(filename);
    Path::new(OsStr::from_bytes(tmp.to_bytes()))
}

#[cfg(not(unix))]
unsafe fn c_path(filename: *const c_char) -> PathBuf {
    let tmp = CStr::from_ptr(filename);
    tmp.to_string_lossy().to_string().into()
}

unsafe fn to_vec(out: &mut *mut u8, outsize: &mut usize, result: Result<Vec<u8>, crate::Error>) -> ErrorCode {
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
