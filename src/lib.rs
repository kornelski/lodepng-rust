#![crate_name = "lodepng"]
#![crate_type = "lib"]

extern crate libc;
extern crate rgb;
extern crate c_vec;

#[allow(non_camel_case_types)]
pub mod ffi;

pub use rgb::RGB;
pub use rgb::RGBA;

use std::os::raw::{c_char, c_uchar, c_uint};
use std::fmt;
use std::mem;
use std::ptr;
use std::cmp;
use std::error;
use std::io;
use std::fs::File;
use std::io::Write;
use std::io::Read;
use c_vec::CVec;
use std::path::Path;
use std::marker::PhantomData;
use std::os::raw::c_void;

pub use ffi::ColorType;
#[doc(hidden)]
pub use ffi::ColorType::{LCT_GREY, LCT_RGB, LCT_PALETTE, LCT_GREY_ALPHA, LCT_RGBA};
pub use ffi::CompressSettings;
use ffi::DecompressSettings;
pub use ffi::Time;
pub use ffi::DecoderSettings;
pub use ffi::FilterStrategy;
#[doc(hidden)]
pub use ffi::FilterStrategy::{LFS_ZERO, LFS_MINSUM, LFS_ENTROPY, LFS_BRUTE_FORCE};
pub use ffi::AutoConvert;
#[doc(hidden)]
pub use ffi::AutoConvert::{LAC_NO, LAC_ALPHA, LAC_AUTO, LAC_AUTO_NO_NIBBLES, LAC_AUTO_NO_PALETTE, LAC_AUTO_NO_NIBBLES_NO_PALETTE};
pub use ffi::EncoderSettings;
pub use ffi::Error;

pub use ffi::Info;
pub use ffi::ColorMode;

impl ColorMode {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn colortype(&self) -> ColorType {
        self.colortype
    }

    pub fn bitdepth(&self) -> u32 {
        self.bitdepth
    }

    pub fn set_bitdepth(&mut self, d: u32) {
        assert!(d >= 1 && d <= 16);
        self.bitdepth = d;
    }

    pub fn palette_clear(&mut self) {
        unsafe {
            ffi::lodepng_palette_clear(self)
        }
    }

    /// add 1 color to the palette
    pub fn palette_add(&mut self, rgba: RGBA<u8>) -> Result<(), Error> {
        unsafe {
            ffi::lodepng_palette_add(self, rgba.r, rgba.g, rgba.b, rgba.a).into()
        }
    }

    pub fn palette(&self) -> &[RGBA<u8>] {
        unsafe {
            std::slice::from_raw_parts(self.palette, self.palettesize)
        }
    }

    pub fn palette_mut(&mut self) -> &mut [RGBA<u8>] {
        unsafe {
            std::slice::from_raw_parts_mut(self.palette as *mut _, self.palettesize)
        }
    }

    /// get the total amount of bits per pixel, based on colortype and bitdepth in the struct
    pub fn bpp(&self) -> u32 {
        unsafe {
            ffi::lodepng_get_bpp(self) as u32
        }
    }

    /// get the amount of color channels used, based on colortype in the struct.
    /// If a palette is used, it counts as 1 channel.
    pub fn channels(&self) -> u32 {
        unsafe {
            ffi::lodepng_get_channels(self) as u32
        }
    }

    /// is it a greyscale type? (only colortype 0 or 4)
    pub fn is_greyscale_type(&self) -> bool {
        unsafe {
            ffi::lodepng_is_greyscale_type(self) != 0
        }
    }

    /// has it got an alpha channel? (only colortype 2 or 6)
    pub fn is_alpha_type(&self) -> bool {
        unsafe {
            ffi::lodepng_is_alpha_type(self) != 0
        }
    }

    /// has it got a palette? (only colortype 3)
    pub fn is_palette_type(&self) -> bool {
        unsafe {
            ffi::lodepng_is_palette_type(self) != 0
        }
    }

    /// only returns true if there is a palette and there is a value in the palette with alpha < 255.
    /// Loops through the palette to check this.
    pub fn has_palette_alpha(&self) -> bool {
        unsafe {
            ffi::lodepng_has_palette_alpha(self) != 0
        }
    }

    /// Check if the given color info indicates the possibility of having non-opaque pixels in the PNG image.
    /// Returns true if the image can have translucent or invisible pixels (it still be opaque if it doesn't use such pixels).
    /// Returns false if the image can only have opaque pixels.
    /// In detail, it returns true only if it's a color type with alpha, or has a palette with non-opaque values,
    /// or if "key_defined" is true.
    pub fn can_have_alpha(&self) -> bool {
        unsafe {
            ffi::lodepng_can_have_alpha(self) != 0
        }
    }

    /// Returns the byte size of a raw image buffer with given width, height and color mode
    pub fn raw_size(&self, w: c_uint, h: c_uint) -> usize {
        unsafe {
            ffi::lodepng_get_raw_size(w, h, self) as usize
        }
    }
}

impl Info {

    /// use this to clear the texts again after you filled them in
    pub fn clear_text(&mut self) {
        unsafe {
            ffi::lodepng_clear_text(self)
        }
    }

    /// push back both texts at once
    pub fn add_text(&mut self, key: *const c_char, str: *const c_char) -> Error {
        unsafe {
            ffi::lodepng_add_text(self, key, str)
        }
    }

    /// use this to clear the itexts again after you filled them in
    pub fn clear_itext(&mut self) {
        unsafe {
            ffi::lodepng_clear_itext(self)
        }
    }

    /// push back the 4 texts of 1 chunk at once
    pub fn add_itext(&mut self, key: *const c_char, langtag: *const c_char, transkey: *const c_char, str: *const c_char) -> Error {
        unsafe {
            ffi::lodepng_add_itext(self, key, langtag, transkey, str)
        }
    }

    pub fn append_chunk(&mut self, position: ChunkPosition, chunk: &Chunk) -> Result<(), Error> {
        unsafe {
            ffi::lodepng_chunk_append(&mut self.unknown_chunks_data[position as usize], &mut self.unknown_chunks_size[position as usize],
                chunk.data).to_result()
        }
    }

    pub fn create_chunk<C: AsRef<[u8]>>(&mut self, position: ChunkPosition, chtype: C, data: &[u8]) -> Result<(), Error> {
        let chtype = chtype.as_ref();
        if chtype.len() != 4 {
            return Error(67).into();
        }
        unsafe {
            ffi::lodepng_chunk_create(&mut self.unknown_chunks_data[position as usize], &mut self.unknown_chunks_size[position as usize],
                data.len() as c_uint, chtype.as_ptr() as *const c_char, data.as_ptr()).to_result()
        }
    }

    pub fn get<Name: AsRef<[u8]>>(&self, index: Name) -> Option<Chunk> {
        let index = index.as_ref();
        return self.unknown_chunks(ChunkPosition::IHDR)
            .chain(self.unknown_chunks(ChunkPosition::PLTE))
            .chain(self.unknown_chunks(ChunkPosition::IDAT))
            .filter(|c| c.is_type(index))
            .next();
    }

    pub fn unknown_chunks(&self, position: ChunkPosition) -> Chunks {
        Chunks {
            data: self.unknown_chunks_data[position as usize],
            len: self.unknown_chunks_size[position as usize],
            _ref: PhantomData,
        }
    }
}

pub struct Chunks<'a> {
    data: *mut u8,
    len: usize,
    _ref: PhantomData<&'a u8>,
}

impl<'a> Iterator for Chunks<'a> {
    type Item = Chunk<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        let chunk_header_len = 12;
        if self.data.is_null() || self.len < chunk_header_len {
            return None;
        }

        let c = Chunk { data: self.data, _ref: PhantomData };
        let l = chunk_header_len + c.len();
        if self.len < l {
            return None;
        }

        self.len -= l;
        unsafe {
            self.data = ffi::lodepng_chunk_next(self.data);
        }

        return Some(c);
    }
}

pub struct State {
    data: ffi::State,
}

impl State {
    pub fn new() -> State {
        unsafe {
            let mut state = State { data: mem::zeroed() };
            ffi::lodepng_state_init(&mut state.data);
            return state;
        }
    }

    pub fn set_auto_convert(&mut self, mode: AutoConvert) {
        self.data.encoder.auto_convert = mode;
    }

    pub fn set_filter_strategy(&mut self, mode: FilterStrategy, palette_filter_zero: bool) {
        self.data.encoder.filter_strategy = mode;
        self.data.encoder.filter_palette_zero = if palette_filter_zero {1} else {0};
    }

    pub unsafe fn set_custom_zlib(&mut self, callback: ffi::custom_zlib_callback, context: *const c_void) {
        self.data.encoder.zlibsettings.custom_zlib = callback;
        self.data.encoder.zlibsettings.custom_context = context;
    }

    pub unsafe fn set_custom_deflate(&mut self, callback: ffi::custom_deflate_callback, context: *const c_void) {
        self.data.encoder.zlibsettings.custom_deflate = callback;
        self.data.encoder.zlibsettings.custom_context = context;
    }

    pub fn info_raw(&self) -> &ColorMode {
        return &self.data.info_raw;
    }

    pub fn info_raw_mut(&mut self) -> &mut ColorMode {
        return &mut self.data.info_raw;
    }

    pub fn info_png(&self) -> &Info {
        return &self.data.info_png;
    }

    pub fn info_png_mut(&mut self) -> &mut Info {
        return &mut self.data.info_png;
    }

    /// whether to convert the PNG to the color type you want. Default: yes
    pub fn color_convert(&mut self, true_or_false: bool) {
        self.data.decoder.color_convert = if true_or_false { 1 } else { 0 };
    }

    /// if false but remember_unknown_chunks is true, they're stored in the unknown chunks.
    pub fn read_text_chunks(&mut self, true_or_false: bool) {
        self.data.decoder.read_text_chunks = if true_or_false { 1 } else { 0 };
    }

    /// store all bytes from unknown chunks in the LodePNGInfo (off by default, useful for a png editor)
    pub fn remember_unknown_chunks(&mut self, true_or_false: bool) {
        self.data.decoder.remember_unknown_chunks = if true_or_false { 1 } else { 0 };
    }

    /// Decompress ICC profile from iCCP chunk
    pub fn get_icc(&self) -> Result<CVec<u8>, Error> {
        let iccp = self.info_png().get("iCCP");
        if iccp.is_none() {
            return Err(Error(89));
        }
        let iccp = iccp.as_ref().unwrap().data();
        if iccp.get(0).cloned().unwrap_or(255) == 0 { // text min length is 1
            return Err(Error(89));
        }

        let name_len = cmp::min(iccp.len(), 80); // skip name
        for i in 0..name_len {
            if iccp[i] == 0 { // string terminator
                if iccp.get(i+1).cloned().unwrap_or(255) != 0 { // compression type
                    return Err(Error(72));
                }
                return zlib_decompress(&iccp[i+2 ..], &self.data.decoder.zlibsettings);
            }
        }
        return Err(Error(75));
    }

    /// Load PNG from buffer using State's settings
    ///
    ///  ```no_run
    ///  # use lodepng::*; let mut state = State::new();
    ///  # let slice = [0u8]; #[allow(unused_variables)] fn do_stuff<T>(buf: T) {}
    ///
    ///  state.info_raw_mut().colortype = LCT_RGBA;
    ///  match state.decode(&slice) {
    ///      Ok(Image::RGBA(with_alpha)) => do_stuff(with_alpha),
    ///      _ => panic!("¯\\_(ツ)_/¯")
    ///  }
    ///  ```
    pub fn decode<Bytes: AsRef<[u8]>>(&mut self, input: Bytes) -> Result<::Image, Error> {
        let input = input.as_ref();
        unsafe {
            let mut out = mem::zeroed();
            let mut w = 0;
            let mut h = 0;

            try!(ffi::lodepng_decode(&mut out, &mut w, &mut h, &mut self.data, input.as_ptr(), input.len() as usize).into());
            Ok(::new_bitmap(out, w as usize, h as usize, self.data.info_raw.colortype, self.data.info_raw.bitdepth))
        }
    }

    pub fn decode_file<P: AsRef<Path>>(&mut self, filepath: P) -> Result<::Image, Error> {
        self.decode(&try!(::load_file(filepath)))
    }

    /// Returns (width, height)
    pub fn inspect(&mut self, input: &[u8]) -> Result<(usize, usize), Error> {
        unsafe {
            let mut w = 0;
            let mut h = 0;
            match ffi::lodepng_inspect(&mut w, &mut h, &mut self.data, input.as_ptr(), input.len() as usize) {
                Error(0) => Ok((w as usize, h as usize)),
                err => Err(err),
            }
        }
    }

    pub fn encode<PixelType: Copy>(&mut self, image: &[PixelType], w: usize, h: usize) -> Result<CVec<u8>, Error> {
        unsafe {
            let mut out = mem::zeroed();
            let mut outsize = 0;

            try!(::with_buffer_for_type(image, w, h, self.data.info_raw.colortype, self.data.info_raw.bitdepth, |ptr| {
                ffi::lodepng_encode(&mut out, &mut outsize, ptr, w as c_uint, h as c_uint, &mut self.data)
            }).into());
            Ok(::cvec_with_free(out, outsize as usize))
        }
    }

    pub fn encode_file<PixelType: Copy, P: AsRef<Path>>(&mut self, filepath: P, image: &[PixelType], w: usize, h: usize) -> Result<(), Error> {
        let buf = try!(self.encode(image, w, h));
        ::save_file(filepath, buf.as_ref())
    }
}

impl Drop for State {
    fn drop(&mut self) {
        unsafe {
            ffi::lodepng_state_cleanup(&mut self.data)
        }
    }
}

impl Clone for State {
    fn clone(&self) -> State {
        unsafe {
            let mut dest = State { data: mem::zeroed() };
            ffi::lodepng_state_copy(&mut dest.data, &self.data);
            return dest;
        }
    }
}

/// Opaque greyscale pixel (acces with `px.0`)
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Grey<ComponentType: Copy>(pub ComponentType);

/// Greyscale pixel with alpha (`px.1` is alpha)
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct GreyAlpha<ComponentType: Copy>(pub ComponentType, pub ComponentType);

impl<T: Default + Copy> Default for Grey<T> {
    fn default() -> Self {
        Grey(Default::default())
    }
}

impl<T: Default + Copy> Default for GreyAlpha<T> {
    fn default() -> Self {
        GreyAlpha(Default::default(), Default::default())
    }
}

/// Bitmap types.
///
/// Images with >=8bpp are stored with pixel per vec element.
/// Images with <8bpp are represented as a bunch of bytes, with multiple pixels per byte.
#[derive(Debug)]
pub enum Image {
    /// Bytes of the image. See bpp how many pixels per element there are
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

impl Error {
    /// Returns an English description of the numerical error code.
    pub fn as_str(&self) -> &'static str {
        unsafe {
            let cstr = std::ffi::CStr::from_ptr(ffi::lodepng_error_text(self.0) as *const _);
            std::str::from_utf8(cstr.to_bytes()).unwrap()
        }
    }

    /// Helper function for the library
    pub fn to_result(self) -> Result<(), Error> {
        match self {
            Error(0) => Ok(()),
            err => Err(err),
        }
    }
}

impl From<Error> for Result<(), Error> {
    fn from(err: Error) -> Self {
        err.to_result()
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} ({})", self.as_str(), self.0)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        self.as_str()
    }
}

#[doc(hidden)]
impl std::convert::From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        match err.kind() {
            io::ErrorKind::NotFound | io::ErrorKind::UnexpectedEof => {Error(78)}
            _ => {Error(79)}
        }
    }
}

/// Position in the file section after…
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum ChunkPosition {
    IHDR = 0,
    PLTE = 1,
    IDAT = 2,
}

/// Reference to a chunk
pub struct Chunk<'a> {
    data: *mut c_uchar,
    _ref: PhantomData<&'a [u8]>,
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
        where T: Send
    {
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

fn required_size(w: usize, h: usize, colortype: ColorType, bitdepth: u32) -> usize {
    colortype.to_color_mode(bitdepth).raw_size(w as c_uint, h as c_uint)
}

unsafe fn cvec_with_free<T>(ptr: *mut T, elts: usize) -> CVec<T>
    where T: Send
{
    CVec::new_with_dtor(ptr, elts, |base: *mut T| {
        self::libc::free(base as *mut libc::c_void);
    })
}

unsafe fn new_bitmap(out: *mut u8, w: usize, h: usize, colortype: ColorType, bitdepth: c_uint) -> Image {
    match (colortype, bitdepth) {
        (LCT_RGBA, 8) => Image::RGBA(Bitmap::from_buffer(out, w, h)),
        (LCT_RGB, 8) => Image::RGB(Bitmap::from_buffer(out, w, h)),
        (LCT_RGBA, 16) => Image::RGBA16(Bitmap::from_buffer(out, w, h)),
        (LCT_RGB, 16) => Image::RGB16(Bitmap::from_buffer(out, w, h)),
        (LCT_GREY, 8) => Image::Grey(Bitmap::from_buffer(out, w, h)),
        (LCT_GREY, 16) => Image::Grey16(Bitmap::from_buffer(out, w, h)),
        (LCT_GREY_ALPHA, 8) => Image::GreyAlpha(Bitmap::from_buffer(out, w, h)),
        (LCT_GREY_ALPHA, 16) => Image::GreyAlpha16(Bitmap::from_buffer(out, w, h)),
        (_, 0) => panic!("Invalid depth"),
        (c,b) => Image::RawData(Bitmap {
            buffer: cvec_with_free(out, required_size(w, h, c, b)),
            width: w,
            height: h,
        }),
    }
}

fn save_file<P: AsRef<Path>>(filepath: P, data: &[u8]) -> Result<(), Error> {
    let mut file = try!(File::create(filepath));
    try!(file.write_all(data));
    return Ok(());
}

fn load_file<P: AsRef<Path>>(filepath: P) -> Result<Vec<u8>, Error> {
    let mut file = try!(File::open(filepath));
    let mut data = Vec::new();
    try!(file.read_to_end(&mut data));
    Ok(data)
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
pub fn decode_memory<Bytes: AsRef<[u8]>>(input: Bytes, colortype: ColorType, bitdepth: c_uint) -> Result<Image, Error> {
    let input = input.as_ref();
    unsafe {
        let mut out = mem::zeroed();
        let mut w = 0;
        let mut h = 0;
        assert!(bitdepth > 0 && bitdepth <= 16);

        try!(ffi::lodepng_decode_memory(&mut out, &mut w, &mut h, input.as_ptr(), input.len() as usize, colortype, bitdepth).into());
        Ok(new_bitmap(out, w as usize, h as usize, colortype, bitdepth))
    }
}

/// Same as `decode_memory`, but always decodes to 32-bit RGBA raw image
pub fn decode32<Bytes: AsRef<[u8]>>(input: Bytes) -> Result<Bitmap<RGBA<u8>>, Error> {
    match try!(decode_memory(input, LCT_RGBA, 8)) {
        Image::RGBA(img) => Ok(img),
        _ => Err(Error(56)), // given output image colortype or bitdepth not supported for color conversion
    }
}

/// Same as `decode_memory`, but always decodes to 24-bit RGB raw image
pub fn decode24<Bytes: AsRef<[u8]>>(input: Bytes) -> Result<Bitmap<RGB<u8>>, Error> {
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
///  ```no_run
///  # use lodepng::*; let filepath = std::path::Path::new("");
///  # fn do_stuff<T>(buf: T) {}
///
///  match decode_file(filepath, LCT_RGBA, 8) {
///      Ok(Image::RGBA(with_alpha)) => do_stuff(with_alpha),
///      Ok(Image::RGB(without_alpha)) => do_stuff(without_alpha),
///      _ => panic!("¯\\_(ツ)_/¯")
///  }
///  ```
pub fn decode_file<P: AsRef<Path>>(filepath: P, colortype: ColorType, bitdepth: c_uint) -> Result<Image, Error> {
    return decode_memory(&try!(::load_file(filepath)), colortype, bitdepth);
}

/// Same as `decode_file`, but always decodes to 32-bit RGBA raw image
pub fn decode32_file<P: AsRef<Path>>(filepath: P) -> Result<Bitmap<RGBA<u8>>, Error> {
    match try!(decode_file(filepath, LCT_RGBA, 8)) {
        Image::RGBA(img) => Ok(img),
        _ => Err(Error(56)),
    }
}

/// Same as `decode_file`, but always decodes to 24-bit RGB raw image
pub fn decode24_file<P: AsRef<Path>>(filepath: P) -> Result<Bitmap<RGB<u8>>, Error> {
    match try!(decode_file(filepath, LCT_RGB, 8)) {
        Image::RGB(img) => Ok(img),
        _ => Err(Error(56)),
    }
}

fn with_buffer_for_type<PixelType: Copy, F>(image: &[PixelType], w: usize, h: usize, colortype: ColorType, bitdepth: u32, mut f: F) -> Error
    where F: FnMut(*const u8) -> Error
{
    let bytes_per_pixel = bitdepth as usize/8;
    assert!(mem::size_of::<PixelType>() <= 4*bytes_per_pixel, "Implausibly large {}-byte pixel data type", mem::size_of::<PixelType>());

    let px_bytes = mem::size_of::<PixelType>();
    let image_bytes = image.len() * px_bytes;
    let required_bytes = required_size(w, h, colortype, bitdepth);

    if image_bytes != required_bytes {
        debug_assert_eq!(image_bytes, required_bytes, "Image is {} bytes large ({}x{}x{}), but needs to be {} ({:?}, {})",
            image_bytes, w,h,px_bytes, required_bytes, colortype, bitdepth);
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
pub fn encode_memory<PixelType: Copy>(image: &[PixelType], w: usize, h: usize, colortype: ColorType, bitdepth: c_uint) -> Result<CVec<u8>, Error> {
    unsafe {
        let mut out = mem::zeroed();
        let mut outsize = 0;

        try!(with_buffer_for_type(image, w, h, colortype, bitdepth, |ptr| {
            ffi::lodepng_encode_memory(&mut out, &mut outsize, ptr, w as c_uint, h as c_uint, colortype, bitdepth)
        }).into());
        Ok(cvec_with_free(out, outsize as usize))
    }
}

/// Same as `encode_memory`, but always encodes from 32-bit RGBA raw image
pub fn encode32<PixelType: Copy>(image: &[PixelType], w: usize, h: usize) -> Result<CVec<u8>, Error> {
    encode_memory(image, w, h, LCT_RGBA, 8)
}

/// Same as `encode_memory`, but always encodes from 24-bit RGB raw image
pub fn encode24<PixelType: Copy>(image: &[PixelType], w: usize, h: usize) -> Result<CVec<u8>, Error> {
    encode_memory(image, w, h, LCT_RGB, 8)
}

/// Converts raw pixel data into a PNG file on disk.
/// Same as the other encode functions, but instead takes a file path as output.
///
/// NOTE: This overwrites existing files without warning!
pub fn encode_file<PixelType: Copy, P: AsRef<Path>>(filepath: P, image: &[PixelType], w: usize, h: usize, colortype: ColorType, bitdepth: c_uint) -> Result<(), Error> {
    let encoded = try!(encode_memory(image, w, h, colortype, bitdepth));
    save_file(filepath, encoded.as_ref())
}

/// Same as `encode_file`, but always encodes from 32-bit RGBA raw image
pub fn encode32_file<PixelType: Copy, P: AsRef<Path>>(filepath: P, image: &[PixelType], w: usize, h: usize) -> Result<(), Error> {
    encode_file(filepath, image, w, h, LCT_RGBA, 8)
}

/// Same as `encode_file`, but always encodes from 24-bit RGB raw image
pub fn encode24_file<PixelType: Copy, P: AsRef<Path>>(filepath: P, image: &[PixelType], w: usize, h: usize) -> Result<(), Error> {
    encode_file(filepath, image, w, h, LCT_RGB, 8)
}

/// Converts from any color type to 24-bit or 32-bit (only)
#[doc(hidden)]
pub fn convert<PixelType: Copy>(input: &[PixelType], mode_out: &mut ColorMode, mode_in: &ColorMode, w: usize, h: usize) -> Result<Image, Error> {
    unsafe {
        let out = mem::zeroed();
        try!(with_buffer_for_type(input, w, h, mode_in.colortype(), mode_in.bitdepth(), |ptr| {
            ffi::lodepng_convert(out, ptr, mode_out, mode_in, w as c_uint, h as c_uint)
        }).into());
        Ok(new_bitmap(out, w, h, mode_out.colortype(), mode_out.bitdepth()))
    }
}

/// Automatically chooses color type that gives smallest amount of bits in the
/// output image, e.g. grey if there are only greyscale pixels, palette if there
/// are less than 256 colors, ...
///
/// The auto_convert parameter is not supported any more.
///
/// updates values of mode with a potentially smaller color model. mode_out should
/// contain the user chosen color model, but will be overwritten with the new chosen one.
#[doc(hidden)]
pub fn auto_choose_color(mode_out: &mut ColorMode, image: *const u8, w: usize, h: usize, mode_in: &ColorMode, _auto_convert: AutoConvert) -> Result<(), Error> {
    unsafe {
        ffi::lodepng_auto_choose_color(mode_out, image, w as c_uint, h as c_uint, mode_in).into()
    }
}

impl<'a> Chunk<'a> {
    pub fn len(&self) -> usize {
        unsafe {
            ffi::lodepng_chunk_length(self.data) as usize
        }
    }

    pub fn name(&self) -> [u8; 4] {
        let mut tmp = [0; 5];
        unsafe {
            ffi::lodepng_chunk_type(&mut tmp,  self.data)
        }
        let mut tmp2 = [0; 4];
        tmp2.copy_from_slice(&tmp[0..4]);
        return tmp2;
    }

    pub fn is_type<C: AsRef<[u8]>>(&self, name: C) -> bool {
        let mut tmp = [0; 5];
        unsafe {
            ffi::lodepng_chunk_type(&mut tmp,  self.data)
        }
        name.as_ref() == &tmp[0..4]
    }

    pub fn is_ancillary(&self) -> c_uchar {
        unsafe {
            ffi::lodepng_chunk_ancillary(self.data)
        }
    }

    pub fn is_private(&self) -> bool {
        unsafe {
            ffi::lodepng_chunk_private(self.data) != 0
        }
    }

    pub fn is_safe_to_copy(&self) -> bool {
        unsafe {
            ffi::lodepng_chunk_safetocopy(self.data) != 0
        }
    }

    pub fn data(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(ffi::lodepng_chunk_data(self.data), self.len())
        }
    }

    #[deprecated(note = "unsound")]
    pub fn data_mut(&mut self) -> &mut [u8] {
        unsafe {
            std::slice::from_raw_parts_mut(ffi::lodepng_chunk_data(self.data), self.len())
        }
    }

    pub fn check_crc(&self) -> bool {
        unsafe {
            ffi::lodepng_chunk_check_crc(&*self.data) == 0
        }
    }

    #[deprecated(note = "unsound")]
    pub fn generate_crc(&mut self) {
        unsafe {
            ffi::lodepng_chunk_generate_crc(self.data)
        }
    }
}

/// Compresses data with Zlib.
/// Zlib adds a small header and trailer around the deflate data.
/// The data is output in the format of the zlib specification.
#[doc(hidden)]
pub fn zlib_compress(input: &[u8], settings: &CompressSettings) -> Result<CVec<u8>, Error> {
    unsafe {
        let mut out = mem::zeroed();
        let mut outsize = 0;

        try!(ffi::lodepng_zlib_compress(&mut out, &mut outsize, input.as_ptr(), input.len() as usize, settings).into());
        Ok(cvec_with_free(out, outsize as usize))
    }
}

fn zlib_decompress(input: &[u8], settings: &DecompressSettings) -> Result<CVec<u8>, Error> {
    unsafe {
        let mut out = mem::zeroed();
        let mut outsize = 0;

        try!(ffi::lodepng_zlib_decompress(&mut out, &mut outsize, input.as_ptr(), input.len() as usize, settings).into());
        Ok(cvec_with_free(out, outsize as usize))
    }
}

/// Compress a buffer with deflate. See RFC 1951.
#[doc(hidden)]
pub fn deflate(input: &[u8], settings: &CompressSettings) -> Result<CVec<u8>, Error> {
    unsafe {
        let mut out = mem::zeroed();
        let mut outsize = 0;

        try!(ffi::lodepng_deflate(&mut out, &mut outsize, input.as_ptr(), input.len() as usize, settings).into());
        Ok(cvec_with_free(out, outsize as usize))
    }
}

impl CompressSettings {
    /// Default compression settings
    pub fn new() -> CompressSettings {
        unsafe {
            let mut settings = mem::zeroed();
            ffi::lodepng_compress_settings_init(&mut settings);
            return settings;
        }
    }
}

impl DecompressSettings {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for DecompressSettings {
    fn default() -> Self {
        Self {
            ignore_adler32: 0,
            custom_zlib: None,
            custom_inflate: None,
            custom_context: ptr::null_mut(),
        }
    }
}

impl EncoderSettings {
    /// Creates encoder settings initialized to defaults
    pub fn new() -> EncoderSettings {
        unsafe {
            let mut settings = mem::zeroed();
            ffi::lodepng_encoder_settings_init(&mut settings);
            settings
        }
    }
}

impl DecoderSettings {
    /// Creates decoder settings initialized to defaults
    pub fn new() -> DecoderSettings {
        unsafe {
            let mut settings = mem::zeroed();
            ffi::lodepng_decoder_settings_init(&mut settings);
            settings
        }
    }
}

#[cfg(test)]
mod test {
    use std::mem;
    use super::*;

    #[test]
    fn pixel_sizes() {
        assert_eq!(4, mem::size_of::<RGBA<u8>>());
        assert_eq!(3, mem::size_of::<RGB<u8>>());
        assert_eq!(2, mem::size_of::<GreyAlpha<u8>>());
        assert_eq!(1, mem::size_of::<Grey<u8>>());
    }

    #[test]
    fn create_and_dstroy1() {
        DecoderSettings::new();
        EncoderSettings::new();
        CompressSettings::new();
    }

    #[test]
    fn create_and_dstroy2() {
        State::new().info_png();
        State::new().info_png_mut();
        State::new().clone().info_raw();
        State::new().clone().info_raw_mut();
    }

    #[test]
    fn test_pal() {
        let mut state = State::new();
        state.info_raw_mut().colortype = LCT_PALETTE;
        assert_eq!(state.info_raw().colortype(), LCT_PALETTE);
        state.info_raw_mut().palette_add(RGBA::new(1,2,3,4)).unwrap();
        state.info_raw_mut().palette_add(RGBA::new(5,6,7,255)).unwrap();
        assert_eq!(&[RGBA{r:1u8,g:2,b:3,a:4},RGBA{r:5u8,g:6,b:7,a:255}], state.info_raw().palette());
        state.info_raw_mut().palette_clear();
        assert_eq!(0, state.info_raw().palette().len());
    }

    #[test]
    fn chunks() {
        let mut state = State::new();
        {
            let info = state.info_png_mut();
            for _ in info.unknown_chunks(ChunkPosition::IHDR) {
                panic!("no chunks yet");
            }

            let testdata = &[1,2,3];
            info.create_chunk(ChunkPosition::PLTE, &[255,0,100,32], testdata).unwrap();
            assert_eq!(1, info.unknown_chunks(ChunkPosition::PLTE).count());

            info.create_chunk(ChunkPosition::IHDR, "foob", testdata).unwrap();
            assert_eq!(1, info.unknown_chunks(ChunkPosition::IHDR).count());
            info.create_chunk(ChunkPosition::IHDR, "foob", testdata).unwrap();
            assert_eq!(2, info.unknown_chunks(ChunkPosition::IHDR).count());

            for _ in info.unknown_chunks(ChunkPosition::IDAT) {}
            let chunk = info.unknown_chunks(ChunkPosition::IHDR).next().unwrap();
            assert_eq!("foob".as_bytes(), chunk.name());
            assert!(chunk.is_type("foob"));
            assert!(!chunk.is_type("foobar"));
            assert!(!chunk.is_type("foo"));
            assert!(!chunk.is_type("FOOB"));
            assert!(chunk.check_crc());
            assert_eq!(testdata, chunk.data());
            info.get("foob").unwrap();
        }

        let img = state.encode(&[0u32], 1, 1).unwrap();
        let mut dec = State::new();
        dec.remember_unknown_chunks(true);
        dec.decode(img).unwrap();
        let chunk = dec.info_png().unknown_chunks(ChunkPosition::IHDR).next().unwrap();
        assert_eq!("foob".as_bytes(), chunk.name());
        dec.info_png().get("foob").unwrap();
    }

    #[test]
    fn read_icc() {
        let mut s = State::new();
        s.remember_unknown_chunks(true);
        let f = s.decode_file("tests/profile.png");
        f.unwrap();
        let icc = s.info_png().get("iCCP").unwrap();
        assert_eq!(275, icc.len());
        assert_eq!("ICC Pro".as_bytes(), &icc.data()[0..7]);

        let data = s.get_icc().unwrap();
        assert_eq!("appl".as_bytes(), &data.as_ref()[4..8]);
    }
}
