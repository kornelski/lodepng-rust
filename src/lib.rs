#![crate_name = "lodepng"]
#![crate_type = "lib"]

extern crate debug;

extern crate libc;
use libc::{c_char, c_uchar, c_uint, c_void, size_t};
use libc::funcs::c95::stdlib;
use std::c_vec::CVec;
use std::fmt;
use std::intrinsics;
use std::io::{File, Open, Read};

pub use ffi::ColorType;
pub use ffi::{LCT_GREY, LCT_RGB, LCT_PALETTE, LCT_GREY_ALPHA, LCT_RGBA};
pub use ffi::ColorMode;
pub use ffi::CompressSettings;
pub use ffi::Time;
pub use ffi::Info;
pub use ffi::DecoderSettings;
pub use ffi::FilterStrategy;
pub use ffi::{LFS_ZERO, LFS_MINSUM, LFS_ENTROPY, LFS_BRUTE_FORCE, LFS_PREDEFINED};
pub use ffi::AutoConvert;
pub use ffi::{LAC_NO, LAC_ALPHA, LAC_AUTO, LAC_AUTO_NO_NIBBLES, LAC_AUTO_NO_PALETTE, LAC_AUTO_NO_NIBBLES_NO_PALETTE};
pub use ffi::EncoderSettings;
pub use ffi::State;
pub use ffi::Error;


#[allow(non_camel_case_types)]
pub mod ffi {
    use libc::{c_char, c_uchar, c_uint, c_void, size_t};
    use std::c_vec::CVec;
    use std::intrinsics;

    #[repr(C)]
    pub struct Error(pub c_uint);

    #[repr(C)]
    pub enum ColorType {
        LCT_GREY = 0,
        LCT_RGB = 2,
        LCT_PALETTE = 3,
        LCT_GREY_ALPHA = 4,
        LCT_RGBA = 6,
    }

    #[repr(C)]
    pub struct ColorMode {
        pub colortype: ColorType,
        pub bitdepth: c_uint,

        pub palette: *const c_uchar,
        pub palettesize: size_t,

        key_defined: c_uint,
        key_r: c_uint,
        key_g: c_uint,
        key_b: c_uint,
    }

    #[repr(C)]
    struct DecompressSettings {
        ignore_adler32: c_uint,
        custom_zlib: *const c_void,
        custom_inflate: *const c_void,
        custom_context: *const c_void,
    }

    #[repr(C)]
    pub struct CompressSettings {
        pub btype: c_uint,
        pub use_lz77: c_uint,
        pub windowsize: c_uint,
        pub minmatch: c_uint,
        pub nicematch: c_uint,
        pub lazymatching: c_uint,

        custom_zlib: *const c_void,
        custom_deflate: *const c_void,
        custom_context: *const c_void,
    }

    #[repr(C)]
    pub struct Time {
        pub year: c_uint,
        pub month: c_uint,
        pub day: c_uint,
        pub hour: c_uint,
        pub minute: c_uint,
        pub second: c_uint,
    }

    #[repr(C)]
    pub struct Info {
        pub compression_method: c_uint,
        pub filter_method: c_uint,
        pub interlace_method: c_uint,
        pub color: ColorMode,

        pub background_defined: c_uint,
        pub background_r: c_uint,
        pub background_g: c_uint,
        pub background_b: c_uint,

        text_num: size_t,
        text_keys: *const *const c_char,
        text_strings: *const *const c_char,

        itext_num: size_t,
        itext_keys: *const *const c_char,
        itext_langtags: *const *const c_char,
        itext_transkeys: *const *const c_char,
        itext_strings: *const *const c_char,

        pub time_defined: c_uint,
        pub time: Time,

        pub phys_defined: c_uint,
        pub phys_x: c_uint,
        pub phys_y: c_uint,
        pub phys_unit: c_uint,

        unknown_chunks_data: [*const c_uchar, ..3],
        unknown_chunks_size: [*const size_t, ..3],
    }

    #[repr(C)]
    pub struct DecoderSettings {
        zlibsettings: DecompressSettings,

        pub ignore_crc: c_uint,

        pub fix_png: c_uint,
        pub color_convert: c_uint,

        read_text_chunks: c_uint,

        remember_unknown_chunks: c_uint,
    }

    #[repr(C)]
    pub enum FilterStrategy {
        LFS_ZERO,
        LFS_MINSUM,
        LFS_ENTROPY,
        LFS_BRUTE_FORCE,
        LFS_PREDEFINED
    }

    #[repr(C)]
    pub enum AutoConvert {
        LAC_NO,
        LAC_ALPHA,
        LAC_AUTO,

        LAC_AUTO_NO_NIBBLES,

        LAC_AUTO_NO_PALETTE,
        LAC_AUTO_NO_NIBBLES_NO_PALETTE
    }

    #[repr(C)]
    pub struct EncoderSettings {
        pub zlibsettings: CompressSettings,

        pub auto_convert: AutoConvert,

        pub filter_palette_zero: c_uint,

        pub filter_strategy: FilterStrategy,

        predefined_filters: *const u8,

        pub force_palette: c_uint,

        add_id: c_uint,

        text_compression: c_uint,
    }

    #[repr(C)]
    pub struct State {
        pub decoder: DecoderSettings,

        pub encoder: EncoderSettings,

        pub info_raw: ColorMode,
        pub info_png: Info,
        pub error: c_uint,
    }

    #[link(name="lodepng", kind="static")]
    extern {
        pub fn lodepng_decode_memory(out: &mut *mut u8, w: &mut c_uint, h: &mut c_uint, input: *const u8, insize: size_t, colortype: ColorType, bitdepth: c_uint) -> Error;
        pub fn lodepng_encode_memory(out: &mut *mut u8, outsize: &mut size_t, image: *const u8, w: c_uint, h: c_uint, colortype: ColorType, bitdepth: c_uint) -> Error;
        pub fn lodepng_error_text(code: Error) -> &'static i8;
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
        pub fn lodepng_chunk_type(chtype: [u8,..5], chunk: *const c_uchar);
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
        pub fn to_result(self) -> Result<(), Error> {
            match self {
                Error(0) => Ok(()),
                err => Err(err),
            }
        }
    }

    impl CompressSettings {
        pub fn new() -> CompressSettings {
            unsafe {
                let mut settings = intrinsics::init();
                lodepng_compress_settings_init(&mut settings);
                return settings;
            }
        }
    }

    impl ColorMode {
        pub fn new() -> ColorMode {
            unsafe {
                let mut mode = intrinsics::init();
                lodepng_color_mode_init(&mut mode);
                return mode;
            }
        }

        pub fn palette_clear(&mut self) {
            unsafe {
                lodepng_palette_clear(self)
            }
        }

        pub fn palette_add(&mut self, r: c_uchar, g: c_uchar, b: c_uchar, a: c_uchar) -> Option<Error> {
            unsafe {
                lodepng_palette_add(self, r, g, b, a).to_result().err()
            }
        }

        pub fn bpp(&self) -> c_uint {
            unsafe {
                lodepng_get_bpp(self)
            }
        }

        pub fn channels(&self) -> c_uint {
            unsafe {
                lodepng_get_channels(self)
            }
        }

        pub fn is_greyscale_type(&self) -> bool {
            unsafe {
                lodepng_is_greyscale_type(self) != 0
            }
        }

        pub fn is_alpha_type(&self) -> bool {
            unsafe {
                lodepng_is_alpha_type(self) != 0
            }
        }

        pub fn is_palette_type(&self) -> bool {
            unsafe {
                lodepng_is_palette_type(self) != 0
            }
        }

        pub fn has_palette_alpha(&self) -> bool {
            unsafe {
                lodepng_has_palette_alpha(self) != 0
            }
        }

        pub fn can_have_alpha(&self) -> bool {
            unsafe {
                lodepng_can_have_alpha(self) != 0
            }
        }

        pub fn raw_size(&self, w: c_uint, h: c_uint) -> uint {
            unsafe {
                lodepng_get_raw_size(w, h, self) as uint
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
        fn clone(&self) -> ColorMode {
            unsafe {
                let mut dest = intrinsics::init();
                lodepng_color_mode_copy(&mut dest, self).to_result().unwrap();
                return dest;
            }
        }
    }

    impl Info {
        pub fn new() -> Info {
            unsafe {
                let mut info = intrinsics::init();
                lodepng_info_init(&mut info);
                return info;
            }
        }

        pub fn clear_text(&mut self) {
            unsafe {
                lodepng_clear_text(self)
            }
        }

        pub fn add_text(&mut self, key: *const c_char, str: *const c_char) -> Error {
            unsafe {
                lodepng_add_text(self, key, str)
            }
        }

        pub fn clear_itext(&mut self) {
            unsafe {
                lodepng_clear_itext(self)
            }
        }

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
        fn clone(&self) -> Info {
            unsafe {
                let mut dest = intrinsics::init();
                lodepng_info_copy(&mut dest, self).to_result().unwrap();
                return dest;
            }
        }
    }

    impl State {
        pub fn new() -> State {
            unsafe {
                let mut state = intrinsics::init();
                lodepng_state_init(&mut state);
                return state;
            }
        }

        pub fn decode(&mut self, input: &[u8]) -> Result<::RawBitmap, Error> {
            unsafe {
                let mut out = intrinsics::init();
                let mut w = 0;
                let mut h = 0;

                let res = lodepng_decode(&mut out, &mut w, &mut h, self, input.as_ptr(), input.len() as size_t);
                ::new_bitmap(res, out, w, h, ::required_size(w, h, self.info_raw.colortype, self.info_raw.bitdepth))
            }
        }

        pub fn inspect(&mut self, input: &[u8]) -> Result<(uint,uint), Error> {
            unsafe {
                let mut w = 0;
                let mut h = 0;
                match lodepng_inspect(&mut w, &mut h, self, input.as_ptr(), input.len() as size_t) {
                    Error(0) => Ok((w as uint,h as uint)),
                    err => Err(err)
                }
            }
        }

        pub fn encode(&mut self, image: &[u8], w: c_uint, h: c_uint) -> Result<CVec<u8>, Error> {
            unsafe {
                let mut out = intrinsics::init();
                let mut outsize = 0;

                let res = ::with_buffer_for_type(image, w, h, self.info_raw.colortype, self.info_raw.bitdepth, |ptr| {
                    lodepng_encode(&mut out, &mut outsize, ptr, w, h, self)
                });
                ::new_buffer(res, out, outsize)
            }
        }

        pub fn encode_file(&mut self, filepath: &Path, image: &[u8], w: c_uint, h: c_uint) -> Result<(), Error> {
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
                let mut dest = intrinsics::init();
                lodepng_state_copy(&mut dest, self);
                return dest;
            }
        }
    }
}

impl fmt::Show for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,"{}",error_text(*self))
    }
}

pub struct Chunk {
    data: *mut c_uchar,
}

pub struct RawBitmap {
    pub buffer: CVec<u8>,
    pub width: c_uint,
    pub height: c_uint,
}

impl fmt::Show for RawBitmap {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{{{} × {}, {:?}}}", self.width, self.height, self.buffer)
    }
}

fn required_size(w: c_uint, h: c_uint, colortype: ColorType, bitdepth: c_uint) -> uint {
    unsafe {
        let color = ColorMode {
            colortype: colortype,
            bitdepth: bitdepth,
            .. intrinsics::init()
        };
        color.raw_size(w, h)
    }
}

unsafe fn new_bitmap(res: Error, out: *mut u8, w: c_uint, h: c_uint, size: uint) -> Result<RawBitmap, Error>  {
    match res {
        Error(0) => Ok(RawBitmap {
            buffer: CVec::new_with_dtor(out, size, proc() {
                stdlib::free(out as *mut c_void);
            }),
            width: w,
            height: h,
        }),
        e => Err(e),
    }
}

unsafe fn new_buffer(res: Error, out: *mut u8, size: size_t) -> Result<CVec<u8>, Error> {
    match res {
        Error(0) => Ok(CVec::new_with_dtor(out, size as uint, proc() {
            stdlib::free(out as *mut c_void);
        })),
        e => Err(e),
    }
}

fn save_file(filepath: &Path, data: &[u8]) -> Result<(), Error> {
    let mut file = match File::create(filepath) {
        Ok(file) => file,
        Err(_) => { return Err(Error(79)) }
    };

    match file.write(data) {
        Ok(_) => Ok(()),
        Err(_) => Err(Error(79)),
    }
}

pub fn decode_memory(input: &[u8], colortype: ColorType, bitdepth: c_uint) -> Result<RawBitmap, Error> {
    unsafe {
        let mut out = intrinsics::init();
        let mut w = 0;
        let mut h = 0;

        let res = ffi::lodepng_decode_memory(&mut out, &mut w, &mut h, input.as_ptr(), input.len() as size_t, colortype, bitdepth);
        new_bitmap(res, out, w, h, required_size(w, h, colortype, bitdepth))
    }
}

pub fn decode32(input: &[u8]) -> Result<RawBitmap, Error> {
    decode_memory(input, LCT_RGBA, 8)
}

pub fn decode24(input: &[u8]) -> Result<RawBitmap, Error> {
    decode_memory(input, LCT_RGB, 8)
}

pub fn decode_file(filepath: &Path, colortype: ColorType, bitdepth: c_uint) -> Result<RawBitmap, Error>  {
    match File::open_mode(filepath, Open, Read).read_to_end() {
        Ok(file) => decode_memory(file.as_slice(), colortype, bitdepth),
        Err(_) => Err(Error(78)),
    }
}

pub fn decode32_file(filepath: &Path) -> Result<RawBitmap, Error> {
    decode_file(filepath, LCT_RGBA, 8)
}

pub fn decode24_file(filepath: &Path) -> Result<RawBitmap, Error> {
    decode_file(filepath, LCT_RGB, 8)
}

fn with_buffer_for_type(image: &[u8], w: c_uint, h: c_uint, colortype: ColorType, bitdepth: c_uint, f: |*const u8| -> Error) -> Error {
    if image.len() != required_size(w, h, colortype, bitdepth) {
        return Error(84);
    }
    f(image.as_ptr())
}

pub fn encode_memory(image: &[u8], w: c_uint, h: c_uint, colortype: ColorType, bitdepth: c_uint) -> Result<CVec<u8>, Error> {
    unsafe {
        let mut out = intrinsics::init();
        let mut outsize = 0;

        let res = with_buffer_for_type(image, w, h, colortype, bitdepth, |ptr| ffi::lodepng_encode_memory(&mut out, &mut outsize, ptr, w, h, colortype, bitdepth));
        new_buffer(res, out, outsize)
    }
}

pub fn encode32(image: &[u8], w: c_uint, h: c_uint) -> Result<CVec<u8>, Error>  {
    encode_memory(image, w, h, LCT_RGBA, 8)
}

pub fn encode24(image: &[u8], w: c_uint, h: c_uint) -> Result<CVec<u8>, Error> {
    encode_memory(image, w, h, LCT_RGB, 8)
}

pub fn encode_file(filepath: &Path, image: &[u8], w: c_uint, h: c_uint, colortype: ColorType, bitdepth: c_uint) -> Result<(), Error> {
    let encoded = try!(encode_memory(image, w, h, colortype, bitdepth));
    save_file(filepath, encoded.as_slice())
}

pub fn encode32_file(filepath: &Path, image: &[u8], w: c_uint, h: c_uint) -> Result<(), Error> {
    encode_file(filepath, image, w, h, LCT_RGBA, 8)
}

pub fn encode24_file(filepath: &Path, image: &[u8], w: c_uint, h: c_uint) -> Result<(), Error> {
    encode_file(filepath, image, w, h, LCT_RGB, 8)
}

pub fn error_text(code: Error) -> &'static str {
    unsafe {
        std::str::raw::c_str_to_static_slice(ffi::lodepng_error_text(code))
    }
}


pub fn convert(input: &[u8], mode_out: &mut ColorMode, mode_in: &ColorMode, w: c_uint, h: c_uint, fix_png: bool) -> Result<RawBitmap, Error> {
    unsafe {
        let out = intrinsics::init();
        let res = with_buffer_for_type(input, w, h, mode_in.colortype, mode_in.bitdepth, |ptr| {
            ffi::lodepng_convert(out, ptr, mode_out, mode_in, w, h, fix_png as c_uint)
        });
        new_bitmap(res, out, w, h, required_size(w, h, mode_out.colortype, mode_out.bitdepth))
    }
}

pub fn decoder_settings_init(settings: &mut DecoderSettings) {
    unsafe {
        ffi::lodepng_decoder_settings_init(settings)
    }
}

pub fn auto_choose_color(mode_out: &mut ColorMode, image: *const u8, w: c_uint, h: c_uint, mode_in: &ColorMode, auto_convert: AutoConvert) -> Result<(), Error> {
    unsafe {
        ffi::lodepng_auto_choose_color(mode_out, image, w, h, mode_in, auto_convert).to_result()
    }
}

pub fn encoder_settings_init(settings: &mut EncoderSettings) {
    unsafe {
        ffi::lodepng_encoder_settings_init(settings)
    }
}

impl Chunk {
    pub fn len(&self) -> uint {
        unsafe {
            ffi::lodepng_chunk_length(&*self.data) as uint
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
                ptr if ptr.is_not_null() => Some(Chunk {data: ptr}),
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

pub fn crc32(buf: &[u8]) -> u32 {
    unsafe {
        ffi::lodepng_crc32(buf.as_ptr(), buf.len() as size_t) as u32
    }
}

pub fn zlib_compress(input: &[u8], settings: &CompressSettings) -> Result<CVec<u8>, Error> {
    unsafe {
        let mut out = intrinsics::init();
        let mut outsize = 0;

        let res = ffi::lodepng_zlib_compress(&mut out, &mut outsize, input.as_ptr(), input.len() as size_t, settings);
        new_buffer(res, out, outsize)
    }
}

pub fn deflate(input: &[u8], settings: &CompressSettings) -> Result<CVec<u8>, Error> {
    unsafe {
        let mut out = intrinsics::init();
        let mut outsize = 0;

        let res = ffi::lodepng_deflate(&mut out, &mut outsize, input.as_ptr(), input.len() as size_t, settings);
        new_buffer(res, out, outsize)
    }
}
