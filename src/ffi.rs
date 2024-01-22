#![cfg_attr(feature = "cargo-clippy", allow(clippy::needless_borrow))]
#![allow(clippy::missing_safety_doc)]
#![allow(non_upper_case_globals)]

use crate::rustimpl::RGBA;
use std::fmt;
use std::io;
use std::num::NonZeroU8;
use std::os::raw::{c_uint, c_void};

#[cfg(not(unix))]
use std::path::PathBuf;

#[cfg(feature = "c_ffi")]
mod functions;

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
    BGRA = 6 | 64,
    /// Not PNG standard, for internal use only. BGR no alpha, 8 bit
    BGR = 2 | 64,
    /// Not PNG standard, for internal use only. BGR no alpha, padded, 8 bit
    BGRX = 3 | 64,
}

impl ColorType {
    /// Bits per pixel
    #[must_use]
    pub fn bpp(&self, bitdepth: u32) -> u32 {
        self.bpp_(bitdepth).get().into()
    }

    #[inline]
    pub(crate) fn bpp_(self, bitdepth: u32) -> NonZeroU8 {
        let bitdepth = bitdepth as u8;
        /*bits per pixel is amount of channels * bits per channel*/
        let ch = self.channels();
        NonZeroU8::new(if ch > 1 {
            ch * if bitdepth == 8 {
                8
            } else {
                16
            }
        } else {
            debug_assert!((bitdepth > 0 && bitdepth <= 8) || bitdepth == 16, "{bitdepth}x{ch}");
            bitdepth.max(1)
        }).unwrap()
    }
}

/// Color mode of an image. Contains all information required to decode the pixel
/// bits to RGBA colors. This information is the same as used in the PNG file
/// format, and is used both for PNG and raw image data in LodePNG.
#[repr(C)]
#[derive(Clone, Debug)]
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

pub type custom_compress_callback = Option<fn(input: &[u8], output: &mut dyn io::Write, context: &CompressSettings) -> Result<(), crate::Error>>;
pub type custom_decompress_callback = Option<fn(input: &[u8], output: &mut dyn io::Write, context: &DecompressSettings) -> Result<(), crate::Error>>;

#[repr(C)]
#[derive(Clone)]
pub struct DecompressSettings {
    pub(crate) custom_zlib: custom_decompress_callback,
    pub(crate) custom_inflate: custom_decompress_callback,
    pub(crate) custom_context: *const c_void,
}

impl fmt::Debug for DecompressSettings {
    #[cold]
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
    /// use custom zlib encoder instead of built in one (default: None).
    ///
    /// You should configure the `flate2` crate to use another zlib instead.
    /// This option will cause unnecessary buffering.
    #[deprecated(note = "use features of the flate2 crate instead")]
    pub custom_zlib: custom_compress_callback,
    /// use custom deflate encoder instead of built in one (default: null)
    /// if custom_zlib is used, custom_deflate is ignored since only the built in
    /// zlib function will call custom_deflate
    ///
    /// You should configure the `flate2` crate to use another zlib instead.
    /// This option will cause unnecessary buffering.
    #[deprecated(note = "use features of the flate2 crate instead")]
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
    #[must_use]
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
    #[allow(clippy::box_collection)]
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
    /// use predefined_filters buffer: you specify the filter type for each scanline.
    /// See `Encoder::set_predefined_filters`.
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

unsafe impl Send for EncoderSettings {}
unsafe impl Sync for EncoderSettings {}

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
#[derive(Debug)]
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
