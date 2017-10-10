
use std::os::raw::{c_char, c_uchar, c_uint, c_void};
use std::mem;
pub use c_vec::CVec;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Error(pub c_uint);

/// Type for `decode`, `encode`, etc. Same as standard PNG color types.
#[repr(C)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ColorType {
    /// greyscale: 1, 2, 4, 8, 16 bit
    LCT_GREY = 0,
    /// RGB: 8, 16 bit
    LCT_RGB = 2,
    /// palette: 1, 2, 4, 8 bit
    LCT_PALETTE = 3,
    /// greyscale with alpha: 8, 16 bit
    LCT_GREY_ALPHA = 4,
    /// RGB with alpha: 8, 16 bit
    LCT_RGBA = 6,
}

/// Color mode of an image. Contains all information required to decode the pixel
/// bits to RGBA colors. This information is the same as used in the PNG file
/// format, and is used both for PNG and raw image data in LodePNG.
#[repr(C)]
#[derive(Debug)]
pub struct ColorMode {
    /// color type, see PNG standard
    pub colortype: ColorType,
    /// bits per sample, see PNG standard
    pub bitdepth: c_uint,

    /// palette (`PLTE` and `tRNS`)
    /// Dynamically allocated with the colors of the palette, including alpha.
    /// When encoding a PNG, to store your colors in the palette of the LodePNGColorMode, first use
    /// lodepng_palette_clear, then for each color use lodepng_palette_add.
    /// If you encode an image without alpha with palette, don't forget to put value 255 in each A byte of the palette.
    ///
    /// When decoding, by default you can ignore this palette, since LodePNG already
    /// fills the palette colors in the pixels of the raw RGBA output.
    ///
    /// The palette is only supported for color type 3.
    pub palette: *const ::RGBA<u8>,
    /// palette size in number of colors (amount of bytes is 4 * `palettesize`)
    pub palettesize: usize,

    /// transparent color key (`tRNS`)
    ///
    /// This color uses the same bit depth as the bitdepth value in this struct, which can be 1-bit to 16-bit.
    /// For greyscale PNGs, r, g and b will all 3 be set to the same.
    ///
    /// When decoding, by default you can ignore this information, since LodePNG sets
    /// pixels with this key to transparent already in the raw RGBA output.
    ///
    /// The color key is only supported for color types 0 and 2.
    key_defined: c_uint,
    key_r: c_uint,
    key_g: c_uint,
    key_b: c_uint,
}

impl ColorType {
    /// Create color mode with given type and bitdepth
    pub fn to_color_mode(&self, bitdepth: c_uint) -> ColorMode {
        unsafe {
            ColorMode {
                colortype: *self,
                bitdepth: bitdepth,
                ..mem::zeroed()
            }
        }
    }
}

#[repr(C)]
pub struct DecompressSettings {
    pub ignore_adler32: c_uint,
    pub custom_zlib: Option<extern "C" fn
                                               (arg1: *mut *mut c_uchar,
                                                arg2: *mut usize,
                                                arg3: *const c_uchar,
                                                arg4: usize,
                                                arg5: *const DecompressSettings)
                                               -> c_uint>,
    pub custom_inflate: Option<extern "C" fn
                                                  (arg1: *mut *mut c_uchar,
                                                   arg2: *mut usize,
                                                   arg3: *const c_uchar,
                                                   arg4: usize,
                                                   arg5: *const DecompressSettings)
                                                  -> c_uint>,
    pub custom_context: *const c_void,
}

/// Settings for zlib compression. Tweaking these settings tweaks the balance between speed and compression ratio.
#[repr(C)]
#[allow(missing_copy_implementations)]
pub struct CompressSettings {
    /// the block type for LZ (0, 1, 2 or 3, see zlib standard). Should be 2 for proper compression.
    pub btype: c_uint,
    /// whether or not to use LZ77. Should be 1 for proper compression.
    pub use_lz77: c_uint,
    /// must be a power of two <= 32768. higher compresses more but is slower. Typical value: 2048.
    pub windowsize: c_uint,
    /// mininum lz77 length. 3 is normally best, 6 can be better for some PNGs. Default: 0
    pub minmatch: c_uint,
    /// stop searching if >= this length found. Set to 258 for best compression. Default: 128
    pub nicematch: c_uint,
    /// use lazy matching: better compression but a bit slower. Default: true
    pub lazymatching: c_uint,
    /// use custom zlib encoder instead of built in one (default: None)
    pub custom_zlib: custom_zlib_callback,
    /// use custom deflate encoder instead of built in one (default: null)
    /// if custom_zlib is used, custom_deflate is ignored since only the built in
    /// zlib function will call custom_deflate
    pub custom_deflate: custom_deflate_callback,
    /// optional custom settings for custom functions
    pub custom_context: *const c_void,
}

pub type custom_zlib_callback = Option<unsafe extern "C" fn
                                               (arg1: &mut *mut c_uchar,
                                                arg2: &mut usize,
                                                arg3: *const c_uchar,
                                                arg4: usize,
                                                arg5: *const CompressSettings)
                                               -> c_uint>;

pub type custom_deflate_callback = Option<unsafe extern "C" fn
                                                  (arg1: &mut *mut c_uchar,
                                                   arg2: &mut usize,
                                                   arg3: *const c_uchar,
                                                   arg4: usize,
                                                   arg5: *const CompressSettings)
                                                  -> c_uint>;
/// The information of a Time chunk in PNG
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Time {
    pub year: c_uint,
    pub month: c_uint,
    pub day: c_uint,
    pub hour: c_uint,
    pub minute: c_uint,
    pub second: c_uint,
}

/// Information about the PNG image, except pixels, width and height
#[repr(C)]
pub struct Info {
    /// compression method of the original file. Always 0.
    pub compression_method: c_uint,
    /// filter method of the original file
    pub filter_method: c_uint,
    /// interlace method of the original file
    pub interlace_method: c_uint,
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
    pub background_defined: c_uint,
    /// red component of suggested background color
    pub background_r: c_uint,
    /// green component of suggested background color
    pub background_g: c_uint,
    /// blue component of suggested background color
    pub background_b: c_uint,

    ///  non-international text chunks (tEXt and zTXt)
    ///
    ///  The `char**` arrays each contain num strings. The actual messages are in
    ///  text_strings, while text_keys are keywords that give a short description what
    ///  the actual text represents, e.g. Title, Author, Description, or anything else.
    ///
    ///  A keyword is minimum 1 character and maximum 79 characters long. It's
    ///  discouraged to use a single line length longer than 79 characters for texts.
    text_num: usize,
    text_keys: *const *const c_char,
    text_strings: *const *const c_char,

    ///  international text chunks (iTXt)
    ///  Similar to the non-international text chunks, but with additional strings
    ///  "langtags" and "transkeys".
    itext_num: usize,
    itext_keys: *const *const c_char,
    itext_langtags: *const *const c_char,
    itext_transkeys: *const *const c_char,
    itext_strings: *const *const c_char,

    /// set to 1 to make the encoder generate a tIME chunk
    pub time_defined: c_uint,
    /// time chunk (tIME)
    pub time: Time,

    /// if 0, there is no pHYs chunk and the values below are undefined, if 1 else there is one
    pub phys_defined: c_uint,
    /// pixels per unit in x direction
    pub phys_x: c_uint,
    /// pixels per unit in y direction
    pub phys_y: c_uint,
    /// may be 0 (unknown unit) or 1 (metre)
    pub phys_unit: c_uint,

    /// unknown chunks
    /// There are 3 buffers, one for each position in the PNG where unknown chunks can appear
    /// each buffer contains all unknown chunks for that position consecutively
    /// The 3 buffers are the unknown chunks between certain critical chunks:
    /// 0: IHDR-`PLTE`, 1: `PLTE`-IDAT, 2: IDAT-IEND
    /// Do not allocate or traverse this data yourself. Use the chunk traversing functions declared
    /// later, such as lodepng_chunk_next and lodepng_chunk_append, to read/write this struct.
    pub unknown_chunks_data: [*mut c_uchar; 3],
    pub unknown_chunks_size: [usize; 3],
}

/// Settings for the decoder. This contains settings for the PNG and the Zlib decoder, but not the Info settings from the Info structs.
#[repr(C)]
pub struct DecoderSettings {
    /// in here is the setting to ignore Adler32 checksums
    pub zlibsettings: DecompressSettings,
    /// ignore CRC checksums
    pub ignore_crc: c_uint,
    pub color_convert: c_uint,
    pub read_text_chunks: c_uint,
    pub remember_unknown_chunks: c_uint,
}

/// automatically use color type with less bits per pixel if losslessly possible. Default: AUTO
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FilterStrategy {
    /// every filter at zero
    LFS_ZERO = 0,
    /// Use filter that gives minumum sum, as described in the official PNG filter heuristic.
    LFS_MINSUM,
    /// Use the filter type that gives smallest Shannon entropy for this scanline. Depending
    /// on the image, this is better or worse than minsum.
    LFS_ENTROPY,
    /// Brute-force-search PNG filters by compressing each filter for each scanline.
    /// Experimental, very slow, and only rarely gives better compression than MINSUM.
    LFS_BRUTE_FORCE,
    /// use predefined_filters buffer: you specify the filter type for each scanline
    LFS_PREDEFINED,
}

/// automatically use color type with less bits per pixel if losslessly possible. Default: LAC_AUTO
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AutoConvert {
    /// use color type user requested
    LAC_NO = 0,
    /// use color type user requested, but if only opaque pixels and RGBA or grey+alpha, use RGB or grey
    LAC_ALPHA,
    /// use PNG color type that can losslessly represent the uncompressed image the smallest possible
    LAC_AUTO,

    /// like AUTO, but do not choose 1, 2 or 4 bit per pixel types.
    /// sometimes a PNG image compresses worse if less than 8 bits per pixels.
    LAC_AUTO_NO_NIBBLES,

    /// like AUTO, but never choose palette color type. For small images, encoding
    /// the palette may take more bytes than what is gained. Note that AUTO also
    /// already prevents encoding the palette for extremely small images, but that may
    /// not be sufficient because due to the compression it cannot predict when to
    /// switch.
    LAC_AUTO_NO_PALETTE,
    LAC_AUTO_NO_NIBBLES_NO_PALETTE,
}

#[repr(C)]
pub struct EncoderSettings {
    /// settings for the zlib encoder, such as window size, ...
    pub zlibsettings: CompressSettings,
    /// how to automatically choose output PNG color type, if at all
    pub auto_convert: AutoConvert,
    /// If true, follows the official PNG heuristic: if the PNG uses a palette or lower than
    /// 8 bit depth, set all filters to zero. Otherwise use the filter_strategy. Note that to
    /// completely follow the official PNG heuristic, filter_palette_zero must be true and
    /// filter_strategy must be LFS_MINSUM
    pub filter_palette_zero: c_uint,
    /// Which filter strategy to use when not using zeroes due to filter_palette_zero.
    /// Set filter_palette_zero to 0 to ensure always using your chosen strategy. Default: LFS_MINSUM
    pub filter_strategy: FilterStrategy,

    /// used if filter_strategy is LFS_PREDEFINED. In that case, this must point to a buffer with
    /// the same length as the amount of scanlines in the image, and each value must <= 5. You
    /// have to cleanup this buffer, LodePNG will never free it. Don't forget that filter_palette_zero
    /// must be set to 0 to ensure this is also used on palette or low bitdepth images
    predefined_filters: *const u8,

    /// force creating a `PLTE` chunk if colortype is 2 or 6 (= a suggested palette).
    /// If colortype is 3, `PLTE` is _always_ created
    pub force_palette: c_uint,
    /// add LodePNG identifier and version as a text chunk, for debugging
    add_id: c_uint,
    /// encode text chunks as zTXt chunks instead of tEXt chunks, and use compression in iTXt chunks
    text_compression: c_uint,
}

/// The settings, state and information for extended encoding and decoding
#[repr(C)]
pub struct State {
    pub decoder: DecoderSettings,

    pub encoder: EncoderSettings,

    /// specifies the format in which you would like to get the raw pixel buffer
    pub info_raw: ColorMode,
    /// info of the PNG image obtained after decoding
    pub info_png: Info,
    error: c_uint,
}

/// Gives characteristics about the colors of the image, which helps decide which color model to use for encoding.
/// Used internally by default if "auto_convert" is enabled. Public because it's useful for custom algorithms.
#[repr(C)]
pub struct ColorProfile {
    /// not greyscale
    pub colored: c_uint,
    /// image is not opaque and color key is possible instead of full alpha
    pub key: c_uint,
    /// key values, always as 16-bit, in 8-bit case the byte is duplicated, e.g. 65535 means 255
    pub key_r: u16,
    pub key_g: u16,
    pub key_b: u16,
    /// image is not opaque and alpha channel or alpha palette required
    pub alpha: c_uint,
    /// amount of colors, up to 257. Not valid if bits == 16.
    pub numcolors: c_uint,
    /// Remembers up to the first 256 RGBA colors, in no particular order
    pub palette: [::RGBA<u8>; 256],
    /// bits per channel (not for palette). 1,2 or 4 for greyscale only. 16 if 16-bit per channel required.
    pub bits: c_uint,
}

#[link(name="lodepng", kind="static")]
extern "C" {
    pub fn lodepng_decode_memory(out: &mut *mut u8, w: &mut c_uint, h: &mut c_uint, input: *const u8, insize: usize, colortype: ColorType, bitdepth: c_uint) -> Error;
    pub fn lodepng_encode_memory(out: &mut *mut u8, outsize: &mut usize, image: *const u8, w: c_uint, h: c_uint, colortype: ColorType, bitdepth: c_uint) -> Error;
    pub fn lodepng_error_text(code: c_uint) -> *const i8;
    pub fn lodepng_compress_settings_init(settings: *mut CompressSettings);
    pub fn lodepng_color_mode_init(info: *mut ColorMode);
    pub fn lodepng_color_mode_cleanup(info: &mut ColorMode);
    pub fn lodepng_color_mode_copy(dest: &mut ColorMode, source: &ColorMode) -> Error;
    pub fn lodepng_palette_clear(info: &mut ColorMode);
    pub fn lodepng_palette_add(info: &mut ColorMode, r: c_uchar, g: c_uchar, b: c_uchar, a: c_uchar) -> Error;
    pub fn lodepng_get_bpp(info: &ColorMode) -> c_uint;
    pub fn lodepng_get_channels(info: &ColorMode) -> c_uint;
    pub fn lodepng_is_greyscale_type(info: &ColorMode) -> c_uint;
    pub fn lodepng_is_alpha_type(info: &ColorMode) -> c_uint;
    pub fn lodepng_is_palette_type(info: &ColorMode) -> c_uint;
    pub fn lodepng_has_palette_alpha(info: &ColorMode) -> c_uint;
    pub fn lodepng_can_have_alpha(info: &ColorMode) -> c_uint;
    pub fn lodepng_get_raw_size(w: c_uint, h: c_uint, color: &ColorMode) -> usize;
    pub fn lodepng_info_init(info: *mut Info);
    pub fn lodepng_info_cleanup(info: &mut Info);
    pub fn lodepng_info_copy(dest: &mut Info, source: &Info) -> Error;
    pub fn lodepng_clear_text(info: &mut Info);
    pub fn lodepng_add_text(info: &mut Info, key: *const c_char, str: *const c_char) -> Error;
    pub fn lodepng_clear_itext(info: &mut Info);
    pub fn lodepng_add_itext(info: &mut Info, key: *const c_char, langtag: *const c_char, transkey: *const c_char, str: *const c_char) -> Error;
    pub fn lodepng_convert(out: *mut u8, input: *const u8, mode_out: &mut ColorMode, mode_in: &ColorMode, w: c_uint, h: c_uint) -> Error;
    pub fn lodepng_decoder_settings_init(settings: *mut DecoderSettings);
    pub fn lodepng_color_profile_init(profile: *mut ColorProfile);
    pub fn lodepng_get_color_profile(profile: &mut ColorProfile, image: *const u8, w: c_uint, h: c_uint, mode_in: &ColorMode) -> Error;
    pub fn lodepng_auto_choose_color(mode_out: &mut ColorMode, image: *const u8, w: c_uint, h: c_uint, mode_in: &ColorMode) -> Error;
    pub fn lodepng_encoder_settings_init(settings: *mut EncoderSettings);
    pub fn lodepng_state_init(state: *mut State);
    pub fn lodepng_state_cleanup(state: &mut State);
    pub fn lodepng_state_copy(dest: &mut State, source: &State);
    pub fn lodepng_decode(out: &mut *mut u8, w: &mut c_uint, h: &mut c_uint, state: &mut State, input: *const u8, insize: usize) -> Error;
    pub fn lodepng_inspect(w: &mut c_uint, h: &mut c_uint, state: &mut State, input: *const u8, insize: usize) -> Error;
    pub fn lodepng_encode(out: &mut *mut u8, outsize: &mut usize, image: *const u8, w: c_uint, h: c_uint, state: &mut State) -> Error;
    pub fn lodepng_chunk_length(chunk: *const c_uchar) -> c_uint;
    pub fn lodepng_chunk_type(chtype: &mut [u8; 5], chunk: *const c_uchar);
    pub fn lodepng_chunk_type_equals(chunk: *const c_uchar, chtype: *const u8) -> c_uchar;
    pub fn lodepng_chunk_ancillary(chunk: *const c_uchar) -> c_uchar;
    pub fn lodepng_chunk_private(chunk: *const c_uchar) -> c_uchar;
    pub fn lodepng_chunk_safetocopy(chunk: *const c_uchar) -> c_uchar;
    pub fn lodepng_chunk_data(chunk: *mut c_uchar) -> *mut c_uchar;
    pub fn lodepng_chunk_check_crc(chunk: *const c_uchar) -> c_uint;
    pub fn lodepng_chunk_generate_crc(chunk: *mut c_uchar);
    pub fn lodepng_chunk_next(chunk: *mut c_uchar) -> *mut c_uchar;
    pub fn lodepng_chunk_append(out: &mut *mut u8, outlength: &mut usize, chunk: *const c_uchar) -> Error;
    pub fn lodepng_chunk_create(out: &mut *mut u8, outlength: &mut usize, length: c_uint, chtype: *const c_char, data: *const u8) -> Error;
    pub fn lodepng_crc32(buf: *const u8, len: usize) -> c_uint;
    pub fn lodepng_zlib_compress(out: &mut *mut u8, outsize: &mut usize, input: *const u8, insize: usize, settings: &CompressSettings) -> Error;
    pub fn lodepng_zlib_decompress(out: &mut *mut u8, outsize: &mut usize, input: *const u8, insize: usize, settings: &DecompressSettings) -> Error;
    pub fn lodepng_deflate(out: &mut *mut u8, outsize: &mut usize, input: *const u8, insize: usize, settings: &CompressSettings) -> Error;
}

