#![crate_name = "lodepng"]
#![crate_type = "lib"]
#![feature(io)]
#![feature(libc)]
#![feature(std_misc)]
#![feature(core)]

extern crate libc;
extern crate c_vec;

use libc::{c_char, c_uchar, c_uint, size_t, c_void, free};
use std::fmt;
use std::mem;
use std::old_io::{File, Open, Read};
use c_vec::CVec;

pub use ffi::ColorType;
pub use ffi::ColorType::{LCT_GREY, LCT_RGB, LCT_PALETTE, LCT_GREY_ALPHA, LCT_RGBA};
pub use ffi::ColorMode;
pub use ffi::CompressSettings;
pub use ffi::Time;
pub use ffi::Info;
pub use ffi::DecoderSettings;
pub use ffi::FilterStrategy;
pub use ffi::FilterStrategy::{LFS_ZERO, LFS_MINSUM, LFS_ENTROPY, LFS_BRUTE_FORCE, LFS_PREDEFINED};
pub use ffi::AutoConvert;
pub use ffi::AutoConvert::{LAC_NO, LAC_ALPHA, LAC_AUTO, LAC_AUTO_NO_NIBBLES, LAC_AUTO_NO_PALETTE, LAC_AUTO_NO_NIBBLES_NO_PALETTE};
pub use ffi::EncoderSettings;
pub use ffi::State;
pub use ffi::Error;

#[allow(non_camel_case_types)]
pub mod ffi {
    use libc::{c_char, c_uchar, c_uint, c_void, size_t};
    use std::mem;
    pub use c_vec::CVec;

    #[repr(C)]
    #[derive(Copy)]
    pub struct Error(pub c_uint);

    impl Error {
        /// Returns an English description of the numerical error code.
        #[stable]
        pub fn as_str(&self) -> &'static str {
            unsafe {
                let bytes = ::std::ffi::c_str_to_bytes(lodepng_error_text(self.0));
                ::std::str::from_utf8_unchecked(bytes)
            }
        }
    }

    /// Type for `decode`, `encode`, etc. Same as standard PNG color types.
    #[repr(C)]
    #[derive(Copy)]
    #[stable]
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
        pub palettesize: size_t,

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
                    .. mem::zeroed()
                }
            }
        }
    }

    #[repr(C)]
    struct DecompressSettings {
    pub ignore_adler32: c_uint,
        pub custom_zlib: ::std::option::Option<extern "C" fn
                                                   (arg1: *mut *mut c_uchar,
                                                    arg2: *mut size_t,
                                                    arg3: *const c_uchar,
                                                    arg4: size_t,
                                                    arg5: *const DecompressSettings)
                                                   -> c_uint>,
        pub custom_inflate: ::std::option::Option<extern "C" fn
                                                      (arg1: *mut *mut c_uchar,
                                                       arg2: *mut size_t,
                                                       arg3: *const c_uchar,
                                                       arg4: size_t,
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
        pub custom_zlib: ::std::option::Option<extern "C" fn
                                                   (arg1: *mut *mut c_uchar,
                                                    arg2: *mut size_t,
                                                    arg3: *const c_uchar,
                                                    arg4: size_t,
                                                    arg5: *const CompressSettings)
                                                   -> c_uint>,
        /// use custom deflate encoder instead of built in one (default: null)
        /// if custom_zlib is used, custom_deflate is ignored since only the built in
        /// zlib function will call custom_deflate
        pub custom_deflate: ::std::option::Option<extern "C" fn
                                                      (arg1: *mut *mut c_uchar,
                                                       arg2: *mut size_t,
                                                       arg3: *const c_uchar,
                                                       arg4: size_t,
                                                       arg5: *const CompressSettings)
                                                      -> c_uint>,
        /// optional custom settings for custom functions
        pub custom_context: *const c_void,
    }

    /// The information of a Time chunk in PNG
    #[repr(C)]
    #[derive(Copy)]
    #[stable]
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
        text_num: size_t,
        text_keys: *const *const c_char,
        text_strings: *const *const c_char,

        ///  international text chunks (iTXt)
        ///  Similar to the non-international text chunks, but with additional strings
        ///  "langtags" and "transkeys".
        itext_num: size_t,
        itext_keys: *const *const c_char,
        itext_langtags: *const *const c_char,
        itext_transkeys: *const *const c_char,
        itext_strings: *const *const c_char,

        /// time chunk (tIME)
        pub time_defined: c_uint, /// set to 1 to make the encoder generate a tIME chunk
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
        unknown_chunks_data: [*const c_uchar; 3],
        unknown_chunks_size: [*const size_t; 3],
    }

    /// Settings for the decoder. This contains settings for the PNG and the Zlib decoder, but not the Info settings from the Info structs.
    #[repr(C)]
    pub struct DecoderSettings {
        /// in here is the setting to ignore Adler32 checksums
        zlibsettings: DecompressSettings,
        /// ignore CRC checksums
        pub ignore_crc: c_uint,

        /// The fix_png setting, if 1, makes the decoder tolerant towards some PNG images
        /// that do not correctly follow the PNG specification. This only supports errors
        /// that are fixable, were found in images that are actually used on the web, and
        /// are silently tolerated by other decoders as well. Currently only one such fix
        /// is implemented: if a palette index is out of bounds given the palette size,
        /// interpret it as opaque black.
        /// By default this value is 0, which makes it stop with an error on such images.
        pub fix_png: c_uint,
        /// whether to convert the PNG to the color type you want. Default: yes
        pub color_convert: c_uint,
        /// if false but remember_unknown_chunks is true, they're stored in the unknown chunks.
        read_text_chunks: c_uint,
        /// store all bytes from unknown chunks in the LodePNGInfo (off by default, useful for a png editor)
        remember_unknown_chunks: c_uint,
    }

    impl DecoderSettings {
        /// Creates decoder settings initialized to defaults
        pub fn new() -> DecoderSettings {
            unsafe {
                let mut settings = ::std::mem::zeroed();
                lodepng_decoder_settings_init(&mut settings);
                settings
            }
        }
    }

    /// automatically use color type with less bits per pixel if losslessly possible. Default: AUTO
    #[repr(C)]
    #[derive(Copy)]
    #[stable]
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
        LFS_PREDEFINED
    }

    /// automatically use color type with less bits per pixel if losslessly possible. Default: LAC_AUTO
    #[repr(C)]
    #[derive(Copy)]
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
        LAC_AUTO_NO_NIBBLES_NO_PALETTE
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

    impl EncoderSettings {
        /// Creates encoder settings initialized to defaults
        pub fn new() -> EncoderSettings {
            unsafe {
                let mut settings = ::std::mem::zeroed();
                lodepng_encoder_settings_init(&mut settings);
                settings
            }
        }
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

    #[link(name="lodepng", kind="static")]
    extern {
        pub fn lodepng_decode_memory(out: &mut *mut u8, w: &mut c_uint, h: &mut c_uint, input: *const u8, insize: size_t, colortype: ColorType, bitdepth: c_uint) -> Error;
        pub fn lodepng_encode_memory(out: &mut *mut u8, outsize: &mut size_t, image: *const u8, w: c_uint, h: c_uint, colortype: ColorType, bitdepth: c_uint) -> Error;
        fn lodepng_error_text(code: c_uint) -> &*const i8;
        pub fn lodepng_compress_settings_init(settings: &mut CompressSettings);
        pub fn lodepng_color_mode_init(info: &mut ColorMode);
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
        pub fn lodepng_get_raw_size(w: c_uint, h: c_uint, color: &ColorMode) -> size_t;
        pub fn lodepng_info_init(info: &mut Info);
        pub fn lodepng_info_cleanup(info: &mut Info);
        pub fn lodepng_info_copy(dest: &mut Info, source: &Info) -> Error;
        pub fn lodepng_clear_text(info: &mut Info);
        pub fn lodepng_add_text(info: &mut Info, key: *const c_char, str: *const c_char) -> Error;
        pub fn lodepng_clear_itext(info: &mut Info);
        pub fn lodepng_add_itext(info: &mut Info, key: *const c_char, langtag: *const c_char, transkey: *const c_char, str: *const c_char) -> Error;
        pub fn lodepng_convert(out: *mut u8, input: *const u8, mode_out: &mut ColorMode, mode_in: &ColorMode, w: c_uint, h: c_uint, fix_png: c_uint) -> Error;
        pub fn lodepng_decoder_settings_init(settings: &mut DecoderSettings);
        pub fn lodepng_auto_choose_color(mode_out: &mut ColorMode, image: *const u8, w: c_uint, h: c_uint, mode_in: &ColorMode, auto_convert: AutoConvert) -> Error;
        pub fn lodepng_encoder_settings_init(settings: &mut EncoderSettings);
        pub fn lodepng_state_init(state: &mut State);
        pub fn lodepng_state_cleanup(state: &mut State);
        pub fn lodepng_state_copy(dest: &mut State, source: &State);
        pub fn lodepng_decode(out: &mut *mut u8, w: &mut c_uint, h: &mut c_uint, state: &mut State, input: *const u8, insize: size_t) -> Error;
        pub fn lodepng_inspect(w: &mut c_uint, h: &mut c_uint, state: &mut State, input: *const u8, insize: size_t) -> Error;
        pub fn lodepng_encode(out: &mut *mut u8, outsize: &mut size_t, image: *const u8, w: c_uint, h: c_uint, state: &mut State) -> Error;
        pub fn lodepng_chunk_length(chunk: *const c_uchar) -> c_uint;
        pub fn lodepng_chunk_type(chtype: [u8; 5], chunk: *const c_uchar);
        pub fn lodepng_chunk_type_equals(chunk: *const c_uchar, chtype: *const u8) -> c_uchar;
        pub fn lodepng_chunk_ancillary(chunk: *const c_uchar) -> c_uchar;
        pub fn lodepng_chunk_private(chunk: *const c_uchar) -> c_uchar;
        pub fn lodepng_chunk_safetocopy(chunk: *const c_uchar) -> c_uchar;
        pub fn lodepng_chunk_data(chunk: *mut c_uchar) -> *mut c_uchar;
        pub fn lodepng_chunk_check_crc(chunk: *const c_uchar) -> c_uint;
        pub fn lodepng_chunk_generate_crc(chunk: *mut c_uchar);
        pub fn lodepng_chunk_next(chunk: *mut c_uchar) -> *mut c_uchar;
        pub fn lodepng_chunk_append(out: &mut *mut u8, outlength: *const size_t, chunk: *const c_uchar) -> Error;
        pub fn lodepng_chunk_create(out: &mut *mut u8, outlength: *const size_t, length: c_uint, chtype: *const c_char, data: *const u8) -> Error;
        pub fn lodepng_crc32(buf: *const u8, len: size_t) -> c_uint;
        pub fn lodepng_zlib_compress(out: &mut *mut u8, outsize: &mut size_t, input: *const u8, insize: size_t, settings: &CompressSettings) -> Error;
        pub fn lodepng_deflate(out: &mut *mut u8, outsize: &mut size_t, input: *const u8, insize: size_t, settings: &CompressSettings) -> Error;
    }

    impl Error {
        /// Helper function for the library
        pub fn to_result(self) -> Result<(), Error> {
            match self {
                Error(0) => Ok(()),
                err => Err(err),
            }
        }
    }

    impl CompressSettings {
        /// Default compression settings
        pub fn new() -> CompressSettings {
            unsafe {
                let mut settings = mem::zeroed();
                lodepng_compress_settings_init(&mut settings);
                return settings;
            }
        }
    }

    impl ColorMode {
        #[stable]
        pub fn new() -> ColorMode {
            unsafe {
                let mut mode = mem::zeroed();
                lodepng_color_mode_init(&mut mode);
                return mode;
            }
        }

        pub fn palette_clear(&mut self) {
            unsafe {
                lodepng_palette_clear(self)
            }
        }

        /// add 1 color to the palette
        #[stable]
        pub fn palette_add(&mut self, r: u8, g: u8, b: u8, a: u8) -> Option<Error> {
            unsafe {
                lodepng_palette_add(self, r, g, b, a).to_result().err()
            }
        }

        /// get the total amount of bits per pixel, based on colortype and bitdepth in the struct
        #[stable]
        pub fn bpp(&self) -> usize {
            unsafe {
                lodepng_get_bpp(self) as usize
            }
        }

        /// get the amount of color channels used, based on colortype in the struct.
        /// If a palette is used, it counts as 1 channel.
        #[stable]
        pub fn channels(&self) -> usize {
            unsafe {
                lodepng_get_channels(self) as usize
            }
        }

        /// is it a greyscale type? (only colortype 0 or 4)
        #[stable]
        pub fn is_greyscale_type(&self) -> bool {
            unsafe {
                lodepng_is_greyscale_type(self) != 0
            }
        }

        /// has it got an alpha channel? (only colortype 2 or 6)
        #[stable]
        pub fn is_alpha_type(&self) -> bool {
            unsafe {
                lodepng_is_alpha_type(self) != 0
            }
        }

        /// has it got a palette? (only colortype 3)
        #[stable]
        pub fn is_palette_type(&self) -> bool {
            unsafe {
                lodepng_is_palette_type(self) != 0
            }
        }

        /// only returns true if there is a palette and there is a value in the palette with alpha < 255.
        /// Loops through the palette to check this.
        #[stable]
        pub fn has_palette_alpha(&self) -> bool {
            unsafe {
                lodepng_has_palette_alpha(self) != 0
            }
        }

        /// Check if the given color info indicates the possibility of having non-opaque pixels in the PNG image.
        /// Returns true if the image can have translucent or invisible pixels (it still be opaque if it doesn't use such pixels).
        /// Returns false if the image can only have opaque pixels.
        /// In detail, it returns true only if it's a color type with alpha, or has a palette with non-opaque values,
        /// or if "key_defined" is true.
        #[stable]
        pub fn can_have_alpha(&self) -> bool {
            unsafe {
                lodepng_can_have_alpha(self) != 0
            }
        }

        /// Returns the byte size of a raw image buffer with given width, height and color mode
        pub fn raw_size(&self, w: c_uint, h: c_uint) -> usize {
            unsafe {
                lodepng_get_raw_size(w, h, self) as usize
            }
        }
    }

    impl Drop for ColorMode {
        fn drop(&mut self) {
            unsafe {
                lodepng_color_mode_cleanup(self)
            }
        }
    }

    impl Clone for ColorMode {
        #[stable]
        fn clone(&self) -> ColorMode {
            unsafe {
                let mut dest = mem::zeroed();
                lodepng_color_mode_copy(&mut dest, self).to_result().unwrap();
                return dest;
            }
        }
    }

    impl Info {
        pub fn new() -> Info {
            unsafe {
                let mut info = mem::zeroed();
                lodepng_info_init(&mut info);
                return info;
            }
        }

        /// use this to clear the texts again after you filled them in
        pub fn clear_text(&mut self) {
            unsafe {
                lodepng_clear_text(self)
            }
        }

        /// push back both texts at once
        #[unstable]
        pub fn add_text(&mut self, key: *const c_char, str: *const c_char) -> Error {
            unsafe {
                lodepng_add_text(self, key, str)
            }
        }

        /// use this to clear the itexts again after you filled them in
        pub fn clear_itext(&mut self) {
            unsafe {
                lodepng_clear_itext(self)
            }
        }

        /// push back the 4 texts of 1 chunk at once
        #[unstable]
        pub fn add_itext(&mut self, key: *const c_char, langtag: *const c_char, transkey: *const c_char, str: *const c_char) -> Error {
            unsafe {
                lodepng_add_itext(self, key, langtag, transkey, str)
            }
        }
    }

    impl Drop for Info {
        fn drop(&mut self) {
            unsafe {
                lodepng_info_cleanup(self)
            }
        }
    }

    impl Clone for Info {
        #[stable]
        fn clone(&self) -> Info {
            unsafe {
                let mut dest = mem::zeroed();
                lodepng_info_copy(&mut dest, self).to_result().unwrap();
                return dest;
            }
        }
    }

    impl State {
        #[stable]
        pub fn new() -> State {
            unsafe {
                let mut state = mem::zeroed();
                lodepng_state_init(&mut state);
                return state;
            }
        }

        /// Load PNG from buffer using State's settings
        ///
        ///     self.info_raw.colortype = LCT_RGBA;
        ///     match try!(state.decode(slice)) {
        ///         Image::RGBA(with_alpha) => do_stuff(with_alpha),
        ///         _ => panic!("¯\\_(ツ)_/¯")
        ///     }
        pub fn decode(&mut self, input: &[u8]) -> Result<::Image, Error> {
            unsafe {
                let mut out = mem::zeroed();
                let mut w = 0;
                let mut h = 0;

                try!(lodepng_decode(&mut out, &mut w, &mut h, self, input.as_ptr(), input.len() as size_t).to_result());
                Ok(::new_bitmap(out, w as usize, h as usize, self.info_raw.colortype, self.info_raw.bitdepth))
            }
        }

        /// Returns (width, height)
        pub fn inspect(&mut self, input: &[u8]) -> Result<(usize, usize), Error> {
            unsafe {
                let mut w = 0;
                let mut h = 0;
                match lodepng_inspect(&mut w, &mut h, self, input.as_ptr(), input.len() as size_t) {
                    Error(0) => Ok((w as usize, h as usize)),
                    err => Err(err)
                }
            }
        }

        pub fn encode<PixelType>(&mut self, image: &[PixelType], w: usize, h: usize) -> Result<CVec<u8>, Error> {
            unsafe {
                let mut out = mem::zeroed();
                let mut outsize = 0;

                try!(::with_buffer_for_type(image, w, h, self.info_raw.colortype, self.info_raw.bitdepth, |ptr| {
                    lodepng_encode(&mut out, &mut outsize, ptr, w as c_uint, h as c_uint, self)
                }).to_result());
                Ok(::new_buffer(out, outsize))
            }
        }

        pub fn encode_file<PixelType>(&mut self, filepath: &Path, image: &[PixelType], w: usize, h: usize) -> Result<(), Error> {
            let buf = try!(self.encode(image, w, h));
            ::save_file(filepath, buf.as_slice())
        }
    }

    impl Drop for State {
        fn drop(&mut self) {
            unsafe {
                lodepng_state_cleanup(self)
            }
        }
    }

    impl Clone for State {
        fn clone(&self) -> State {
            unsafe {
                let mut dest = mem::zeroed();
                lodepng_state_copy(&mut dest, self);
                return dest;
            }
        }
    }
}

/// `RGBA<T>` with `T` appropriate for bit depth (`u8`, `u16`)
#[repr(C)]
#[derive(Copy)]
pub struct RGBA<ComponentType> {
    pub r: ComponentType,
    pub g: ComponentType,
    pub b: ComponentType,
    /// 0 = transparent, 255 = opaque
    pub a: ComponentType,
}

/// `RGB<T>` with `T` appropriate for bit depth (`u8`, `u16`)
#[repr(C)]
#[derive(Copy)]
pub struct RGB<ComponentType> {
    pub r: ComponentType,
    pub g: ComponentType,
    pub b: ComponentType,
}

/// Opaque greyscale pixel (acces with `px.0`)
pub struct Grey<ComponentType>(ComponentType);

/// Greyscale pixel with alpha (`px.1` is alpha)
pub struct GreyAlpha<ComponentType>(ComponentType, ComponentType);

/// Images with <8bpp are represented as a bunch of bytes
///
/// To safely convert RGB/RGBA see `Vec::map_in_place`,
/// or use `transmute()`
pub enum Image {
    RawData(Bitmap<u8>),
    Grey(Bitmap<Grey<u8>>),
    Grey16(Bitmap<Grey<u16>>),
    GreyAlpha(Bitmap<GreyAlpha<u8>>),
    GreyAlpha16(Bitmap<GreyAlpha<u16>>),
    RGBA(Bitmap<RGBA<u8>>),
    RGB(Bitmap<RGB<u8>>),
    RGBA16(Bitmap<RGBA<u16>>),
    RGB16(Bitmap<RGB<u16>>),
}

impl<T: fmt::Display> fmt::Display for RGBA<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,"rgba({},{},{},{})", self.r,self.g,self.b,self.a)
    }
}

impl<T: fmt::Display> fmt::Display for RGB<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,"rgb({},{},{})", self.r,self.g,self.b)
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,"{}", self.as_str())
    }
}

#[allow(missing_copy_implementations)]
#[unstable]
pub struct Chunk {
    data: *mut c_uchar,
}

/// Low-level representation of an image
pub struct Bitmap<PixelType> {
    /// Raw bitmap memory. Layout depends on color mode and bitdepth used to create it.
    ///
    /// * For RGB/RGBA images one element is one pixel.
    /// * For <8bpp images pixels are packed, so raw bytes are exposed and you need to do bit-twiddling youself
    pub buffer: CVec<PixelType>,
    /// Width in pixels
    pub width: usize,
    /// Height in pixels
    pub height: usize,
}

impl<T: Send> Bitmap<T> {
    unsafe fn from_buffer(out: *mut u8, w: usize, h: usize) -> Bitmap<T>
        where T: Send {
        Bitmap::<T> {
            buffer: cvec_with_free(mem::transmute(out), w * h),
            width: w,
            height: h,
        }
    }
}

impl<PixelType> fmt::Debug for Bitmap<PixelType> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{{{} × {} Bitmap}}", self.width, self.height)
    }
}

fn required_size(w: usize, h: usize, colortype: ColorType, bitdepth: c_uint) -> usize {
    colortype.to_color_mode(bitdepth).raw_size(w as c_uint, h as c_uint)
}

unsafe fn cvec_with_free<T>(ptr: *mut T, elts: usize) -> CVec<T>
    where T: Send {
    let uniq_ptr = ::std::ptr::Unique(ptr);
    CVec::new_with_dtor(ptr, elts, move || free(uniq_ptr.0 as *mut c_void))
}

unsafe fn new_bitmap(out: *mut u8, w: usize, h: usize, colortype: ColorType, bitdepth: c_uint) -> Image  {
    match (colortype, bitdepth) {
        (LCT_RGBA, 8) =>Image::RGBA(Bitmap::from_buffer(out, w, h)),
        (LCT_RGB, 8) => Image::RGB(Bitmap::from_buffer(out, w, h)),
        (LCT_RGBA, 16) => Image::RGBA16(Bitmap::from_buffer(out, w, h)),
        (LCT_RGB, 16) => Image::RGB16(Bitmap::from_buffer(out, w, h)),
        (LCT_GREY, 8) => Image::Grey(Bitmap::from_buffer(out, w, h)),
        (LCT_GREY, 16) => Image::Grey16(Bitmap::from_buffer(out, w, h)),
        (LCT_GREY_ALPHA, 8) => Image::GreyAlpha(Bitmap::from_buffer(out, w, h)),
        (LCT_GREY_ALPHA, 16) => Image::GreyAlpha16(Bitmap::from_buffer(out, w, h)),
        (c,b) => Image::RawData(Bitmap {
            buffer: cvec_with_free(out, required_size(w, h, c, b)),
            width: w,
            height: h,
        }),
    }
}

unsafe fn new_buffer(out: *mut u8, size: size_t) -> CVec<u8> {
    CVec::new(out, size as usize)
}

fn save_file(filepath: &Path, data: &[u8]) -> Result<(), Error> {
    if let Ok(mut file) = File::create(filepath) {
        if file.write_all(data).is_ok() {
            return Ok(());
        }
    }
    return Err(Error(79));
}

/// Converts PNG data in memory to raw pixel data.
///
/// `decode32` and `decode24` are more convenient if you want specific image format.
///
/// See `ffi::State::decode()` for advanced decoding.
///
/// * `in`: Memory buffer with the PNG file.
/// * `colortype`: the desired color type for the raw output image. See `ColorType`.
/// * `bitdepth`: the desired bit depth for the raw output image. 1, 2, 4, 8 or 16. Typically 8.
pub fn decode_memory(input: &[u8], colortype: ColorType, bitdepth: c_uint) -> Result<Image, Error> {
    unsafe {
        let mut out = mem::zeroed();
        let mut w = 0;
        let mut h = 0;

        try!(ffi::lodepng_decode_memory(&mut out, &mut w, &mut h, input.as_ptr(), input.len() as size_t, colortype, bitdepth).to_result());
        Ok(new_bitmap(out, w as usize, h as usize, colortype, bitdepth))
    }
}

/// Same as `decode_memory`, but always decodes to 32-bit RGBA raw image
pub fn decode32(input: &[u8]) -> Result<Bitmap<RGBA<u8>>, Error> {
    match try!(decode_memory(input, LCT_RGBA, 8)) {
        Image::RGBA(img) => Ok(img),
        _ => Err(Error(56)), // given output image colortype or bitdepth not supported for color conversion
    }
}

/// Same as `decode_memory`, but always decodes to 24-bit RGB raw image
pub fn decode24(input: &[u8]) -> Result<Bitmap<RGB<u8>>, Error> {
    match try!(decode_memory(input, LCT_RGB, 8)) {
        Image::RGB(img) => Ok(img),
        _ => Err(Error(56)),
    }
}

/// Load PNG from disk, from file with given name.
/// Same as the other decode functions, but instead takes a file path as input.
///
/// `decode32_file` and `decode24_file` are more convenient if you want specific image format.
///
/// There's also `ffi::State::decode()` if you'd like to set more settings.
///
///     match try!(decode_file(filepath, LCT_RGBA, 8)) {
///         Image::RGBA(with_alpha) => do_stuff(with_alpha),
///         Image::RGB(without_alpha) => do_stuff(without_alpha),
///         _ => panic!("¯\\_(ツ)_/¯")
///     }
pub fn decode_file(filepath: &Path, colortype: ColorType, bitdepth: c_uint) -> Result<Image, Error>  {
    match File::open_mode(filepath, Open, Read).read_to_end() {
        Ok(file) => decode_memory(&file[..], colortype, bitdepth),
        Err(_) => Err(Error(78)),
    }
}

/// Same as `decode_file`, but always decodes to 32-bit RGBA raw image
pub fn decode32_file(filepath: &Path) -> Result<Bitmap<RGBA<u8>>, Error> {
    match try!(decode_file(filepath, LCT_RGBA, 8)) {
        Image::RGBA(img) => Ok(img),
        _ => Err(Error(56)),
    }
}

/// Same as `decode_file`, but always decodes to 24-bit RGB raw image
pub fn decode24_file(filepath: &Path) -> Result<Bitmap<RGB<u8>>, Error> {
    match try!(decode_file(filepath, LCT_RGB, 8)) {
        Image::RGB(img) => Ok(img),
        _ => Err(Error(56)),
    }
}

fn with_buffer_for_type<PixelType, F>(image: &[PixelType], w: usize, h: usize, colortype: ColorType, bitdepth: c_uint, mut f: F) -> Error
    where F: FnMut(*const u8) -> Error
{
    if image.len() * mem::size_of::<PixelType>() != required_size(w, h, colortype, bitdepth) {
        return Error(84);
    }

    f(unsafe {mem::transmute(image.as_ptr())})
}

/// Converts raw pixel data into a PNG image in memory. The colortype and bitdepth
/// of the output PNG image cannot be chosen, they are automatically determined
/// by the colortype, bitdepth and content of the input pixel data.
///
/// Note: for 16-bit per channel colors, needs big endian format like PNG does.
///
/// * `image`: The raw pixel data to encode. The size of this buffer should be `w` * `h` * (bytes per pixel), bytes per pixel depends on colortype and bitdepth.
/// * `w`: width of the raw pixel data in pixels.
/// * `h`: height of the raw pixel data in pixels.
/// * `colortype`: the color type of the raw input image. See `ColorType`.
/// * `bitdepth`: the bit depth of the raw input image. 1, 2, 4, 8 or 16. Typically 8.
pub fn encode_memory<PixelType>(image: &[PixelType], w: usize, h: usize, colortype: ColorType, bitdepth: c_uint) -> Result<CVec<u8>, Error> {
    unsafe {
        let mut out = mem::zeroed();
        let mut outsize = 0;

        try!(with_buffer_for_type(image, w, h, colortype, bitdepth, |ptr| {
            ffi::lodepng_encode_memory(&mut out, &mut outsize, ptr, w as c_uint, h as c_uint, colortype, bitdepth)
        }).to_result());
        Ok(new_buffer(out, outsize))
    }
}

/// Same as `encode_memory`, but always encodes from 32-bit RGBA raw image
pub fn encode32<PixelType>(image: &[PixelType], w: usize, h: usize) -> Result<CVec<u8>, Error>  {
    encode_memory(image, w, h, LCT_RGBA, 8)
}

/// Same as `encode_memory`, but always encodes from 24-bit RGB raw image
pub fn encode24<PixelType>(image: &[PixelType], w: usize, h: usize) -> Result<CVec<u8>, Error> {
    encode_memory(image, w, h, LCT_RGB, 8)
}

/// Converts raw pixel data into a PNG file on disk.
/// Same as the other encode functions, but instead takes a file path as output.
///
/// NOTE: This overwrites existing files without warning!
pub fn encode_file<PixelType>(filepath: &Path, image: &[PixelType], w: usize, h: usize, colortype: ColorType, bitdepth: c_uint) -> Result<(), Error> {
    let encoded = try!(encode_memory(image, w, h, colortype, bitdepth));
    save_file(filepath, encoded.as_slice())
}

/// Same as `encode_file`, but always encodes from 32-bit RGBA raw image
pub fn encode32_file<PixelType>(filepath: &Path, image: &[PixelType], w: usize, h: usize) -> Result<(), Error> {
    encode_file(filepath, image, w, h, LCT_RGBA, 8)
}

/// Same as `encode_file`, but always encodes from 24-bit RGB raw image
pub fn encode24_file<PixelType>(filepath: &Path, image: &[PixelType], w: usize, h: usize) -> Result<(), Error> {
    encode_file(filepath, image, w, h, LCT_RGB, 8)
}

/// Converts from any color type to 24-bit or 32-bit (only)
pub fn convert<PixelType>(input: &[PixelType], mode_out: &mut ColorMode, mode_in: &ColorMode, w: usize, h: usize, fix_png: bool) -> Result<Image, Error> {
    unsafe {
        let out = mem::zeroed();
        try!(with_buffer_for_type(input, w, h, mode_in.colortype, mode_in.bitdepth, |ptr| {
            ffi::lodepng_convert(out, ptr, mode_out, mode_in, w as c_uint, h as c_uint, fix_png as c_uint)
        }).to_result());
        Ok(new_bitmap(out, w, h, mode_out.colortype, mode_out.bitdepth))
    }
}

/// Automatically chooses color type that gives smallest amount of bits in the
/// output image, e.g. grey if there are only greyscale pixels, palette if there
/// are less than 256 colors, ...
///
/// The auto_convert parameter allows limiting it to not use palette.
///
/// updates values of mode with a potentially smaller color model. mode_out should
/// contain the user chosen color model, but will be overwritten with the new chosen one.
pub fn auto_choose_color(mode_out: &mut ColorMode, image: *const u8, w: usize, h: usize, mode_in: &ColorMode, auto_convert: AutoConvert) -> Result<(), Error> {
    unsafe {
        ffi::lodepng_auto_choose_color(mode_out, image, w as c_uint, h as c_uint, mode_in, auto_convert).to_result()
    }
}

impl Chunk {
    pub fn len(&self) -> usize {
        unsafe {
            ffi::lodepng_chunk_length(&*self.data) as usize
        }
    }

    pub fn is_ancillary(&self) -> c_uchar {
        unsafe {
            ffi::lodepng_chunk_ancillary(&*self.data)
        }
    }

    pub fn is_private(&self) -> bool {
        unsafe {
            ffi::lodepng_chunk_private(&*self.data) != 0
        }
    }

    pub fn is_safetocopy(&self) -> bool {
        unsafe {
            ffi::lodepng_chunk_safetocopy(&*self.data) != 0
        }
    }

    pub fn data(&self) -> *mut c_uchar {
        unsafe {
            ffi::lodepng_chunk_data(self.data)
        }
    }

    pub fn check_crc(&self) -> bool {
        unsafe {
            ffi::lodepng_chunk_check_crc(&*self.data) != 0
        }
    }

    pub fn generate_crc(&mut self) {
        unsafe {
            ffi::lodepng_chunk_generate_crc(self.data)
        }
    }

    pub fn next(&self) -> Option<Chunk> {
        unsafe {
            match ffi::lodepng_chunk_next(self.data) {
                ptr if !ptr.is_null() => Some(Chunk {data: ptr}),
                _ => None,
            }
        }
    }

    pub fn append(&self, out: &mut *mut u8, outlength: *const size_t) -> Result<(), Error> {
        unsafe {
            ffi::lodepng_chunk_append(out, outlength, &*self.data).to_result()
        }
    }

    pub fn create(out: &mut *mut u8, outlength: *const size_t, length: c_uint, chtype: *const c_char, data: *const u8) -> Result<(), Error> {
        unsafe {
            ffi::lodepng_chunk_create(out, outlength, length, chtype, data).to_result()
        }
    }
}

/// Compresses data with Zlib.
/// Zlib adds a small header and trailer around the deflate data.
/// The data is output in the format of the zlib specification.
#[unstable]
pub fn zlib_compress(input: &[u8], settings: &CompressSettings) -> Result<CVec<u8>, Error> {
    unsafe {
        let mut out = mem::zeroed();
        let mut outsize = 0;

        try!(ffi::lodepng_zlib_compress(&mut out, &mut outsize, input.as_ptr(), input.len() as size_t, settings).to_result());
        Ok(new_buffer(out, outsize))
    }
}

/// Compress a buffer with deflate. See RFC 1951.
#[unstable]
pub fn deflate(input: &[u8], settings: &CompressSettings) -> Result<CVec<u8>, Error> {
    unsafe {
        let mut out = mem::zeroed();
        let mut outsize = 0;

        try!(ffi::lodepng_deflate(&mut out, &mut outsize, input.as_ptr(), input.len() as size_t, settings).to_result());
        Ok(new_buffer(out, outsize))
    }
}
