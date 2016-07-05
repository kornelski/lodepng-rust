#![crate_name = "lodepng"]
#![crate_type = "lib"]

extern crate libc;
extern crate rgb;
extern crate c_vec;

#[allow(non_camel_case_types)]
pub mod ffi;

pub use rgb::RGB;
pub use rgb::RGBA;

use libc::{c_char, c_uchar, c_uint, size_t, c_void, free};
use std::fmt;
use std::mem;
use std::io;
use std::fs::File;
use std::io::Write;
use std::io::Read;
use c_vec::CVec;
use std::path::Path;

pub use ffi::ColorType;
#[doc(hidden)]
pub use ffi::ColorType::{LCT_GREY, LCT_RGB, LCT_PALETTE, LCT_GREY_ALPHA, LCT_RGBA};
pub use ffi::CompressSettings;
pub use ffi::Time;
pub use ffi::DecoderSettings;
pub use ffi::FilterStrategy;
#[doc(hidden)]
pub use ffi::FilterStrategy::{LFS_ZERO, LFS_MINSUM, LFS_ENTROPY, LFS_BRUTE_FORCE, LFS_PREDEFINED};
pub use ffi::AutoConvert;
#[doc(hidden)]
pub use ffi::AutoConvert::{LAC_NO, LAC_ALPHA, LAC_AUTO, LAC_AUTO_NO_NIBBLES, LAC_AUTO_NO_PALETTE, LAC_AUTO_NO_NIBBLES_NO_PALETTE};
pub use ffi::EncoderSettings;
pub use ffi::Error;

pub struct ColorMode {
    data: ffi::ColorMode,
}

impl ColorMode {
    pub fn new() -> ColorMode {
        unsafe {
            let mut mode = mem::zeroed();
            ffi::lodepng_color_mode_init(&mut mode);
            return ColorMode { data: mode };
        }
    }

    pub fn colortype(&self) -> ColorType {
        self.data.colortype
    }

    pub fn bitdepth(&self) -> u32 {
        self.data.bitdepth
    }

    pub fn palette_clear(&mut self) {
        unsafe {
            ffi::lodepng_palette_clear(&mut self.data)
        }
    }

    /// add 1 color to the palette
    pub fn palette_add(&mut self, r: u8, g: u8, b: u8, a: u8) -> Option<Error> {
        unsafe {
            ffi::lodepng_palette_add(&mut self.data, r, g, b, a).to_result().err()
        }
    }

    /// get the total amount of bits per pixel, based on colortype and bitdepth in the struct
    pub fn bpp(&self) -> u32 {
        unsafe {
            ffi::lodepng_get_bpp(&self.data) as u32
        }
    }

    /// get the amount of color channels used, based on colortype in the struct.
    /// If a palette is used, it counts as 1 channel.
    pub fn channels(&self) -> u32 {
        unsafe {
            ffi::lodepng_get_channels(&self.data) as u32
        }
    }

    /// is it a greyscale type? (only colortype 0 or 4)
    pub fn is_greyscale_type(&self) -> bool {
        unsafe {
            ffi::lodepng_is_greyscale_type(&self.data) != 0
        }
    }

    /// has it got an alpha channel? (only colortype 2 or 6)
    pub fn is_alpha_type(&self) -> bool {
        unsafe {
            ffi::lodepng_is_alpha_type(&self.data) != 0
        }
    }

    /// has it got a palette? (only colortype 3)
    pub fn is_palette_type(&self) -> bool {
        unsafe {
            ffi::lodepng_is_palette_type(&self.data) != 0
        }
    }

    /// only returns true if there is a palette and there is a value in the palette with alpha < 255.
    /// Loops through the palette to check this.
    pub fn has_palette_alpha(&self) -> bool {
        unsafe {
            ffi::lodepng_has_palette_alpha(&self.data) != 0
        }
    }

    /// Check if the given color info indicates the possibility of having non-opaque pixels in the PNG image.
    /// Returns true if the image can have translucent or invisible pixels (it still be opaque if it doesn't use such pixels).
    /// Returns false if the image can only have opaque pixels.
    /// In detail, it returns true only if it's a color type with alpha, or has a palette with non-opaque values,
    /// or if "key_defined" is true.
    pub fn can_have_alpha(&self) -> bool {
        unsafe {
            ffi::lodepng_can_have_alpha(&self.data) != 0
        }
    }

    /// Returns the byte size of a raw image buffer with given width, height and color mode
    pub fn raw_size(&self, w: c_uint, h: c_uint) -> usize {
        unsafe {
            ffi::lodepng_get_raw_size(w, h, &self.data) as usize
        }
    }
}

impl Drop for ColorMode {
    fn drop(&mut self) {
        unsafe {
            ffi::lodepng_color_mode_cleanup(&mut self.data)
        }
    }
}

impl Clone for ColorMode {
    fn clone(&self) -> ColorMode {
        unsafe {
            let mut dest = ColorMode { data: mem::zeroed() };
            ffi::lodepng_color_mode_copy(&mut dest.data, &self.data).to_result().unwrap();
            return dest;
        }
    }
}


pub struct Info {
    data: ffi::Info,
}

impl Info {
    pub fn new() -> Info {
        unsafe {
            let mut info = Info { data: mem::zeroed() };
            ffi::lodepng_info_init(&mut info.data);
            return info;
        }
    }

    /// use this to clear the texts again after you filled them in
    pub fn clear_text(&mut self) {
        unsafe {
            ffi::lodepng_clear_text(&mut self.data)
        }
    }

    /// push back both texts at once
    pub fn add_text(&mut self, key: *const c_char, str: *const c_char) -> Error {
        unsafe {
            ffi::lodepng_add_text(&mut self.data, key, str)
        }
    }

    /// use this to clear the itexts again after you filled them in
    pub fn clear_itext(&mut self) {
        unsafe {
            ffi::lodepng_clear_itext(&mut self.data)
        }
    }

    /// push back the 4 texts of 1 chunk at once
    pub fn add_itext(&mut self, key: *const c_char, langtag: *const c_char, transkey: *const c_char, str: *const c_char) -> Error {
        unsafe {
            ffi::lodepng_add_itext(&mut self.data, key, langtag, transkey, str)
        }
    }
}

impl Drop for Info {
    fn drop(&mut self) {
        unsafe {
            ffi::lodepng_info_cleanup(&mut self.data)
        }
    }
}

impl Clone for Info {
    fn clone(&self) -> Info {
        unsafe {
            let mut dest = Info{ data:mem::zeroed() };
            ffi::lodepng_info_copy(&mut dest.data, &self.data).to_result().unwrap();
            return dest;
        }
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

    pub fn info_raw(&mut self) -> &mut ffi::ColorMode {
        return &mut self.data.info_raw;
    }

    pub fn info_png(&mut self) -> &mut ffi::Info {
        return &mut self.data.info_png;
    }

    /// Load PNG from buffer using State's settings
    ///
    ///  ```no_run
    ///  # use lodepng::*; let mut state = State::new();
    ///  # let slice = [0u8]; fn do_stuff<T>(buf: T) {}
    ///
    ///  state.info_raw().colortype = LCT_RGBA;
    ///  match state.decode(&slice) {
    ///      Ok(Image::RGBA(with_alpha)) => do_stuff(with_alpha),
    ///      _ => panic!("¯\\_(ツ)_/¯")
    ///  }
    ///  ```
    pub fn decode(&mut self, input: &[u8]) -> Result<::Image, Error> {
        unsafe {
            let mut out = mem::zeroed();
            let mut w = 0;
            let mut h = 0;

            try!(ffi::lodepng_decode(&mut out, &mut w, &mut h, &mut self.data, input.as_ptr(), input.len() as size_t).to_result());
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
            match ffi::lodepng_inspect(&mut w, &mut h, &mut self.data, input.as_ptr(), input.len() as size_t) {
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
            }).to_result());
            Ok(::cvec_with_free(out, outsize as usize))
        }
    }

    pub fn encode_file<PixelType: Copy, P: AsRef<Path>>(&mut self, filepath: P, image: &[PixelType], w: usize, h: usize) -> Result<(), Error> {
        let buf = try!(self.encode(image, w, h));
        ::save_file(filepath, buf.as_cslice().as_ref())
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
#[derive(Debug)]
pub struct Grey<ComponentType>(ComponentType);

/// Greyscale pixel with alpha (`px.1` is alpha)
#[derive(Debug)]
pub struct GreyAlpha<ComponentType>(ComponentType, ComponentType);

/// Bitmap types. Also contains valid values for `<PixelType>`
///
/// Images with <8bpp are represented as a bunch of bytes.
///
/// To safely convert RGB/RGBA see `Vec::map_in_place`,
/// or use `transmute()`
#[derive(Debug)]
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

#[doc(hidden)]
impl std::convert::From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        match err.kind() {
            io::ErrorKind::NotFound | io::ErrorKind::UnexpectedEof => {Error(78)}
            _ => {Error(79)}
        }
    }
}

#[allow(missing_copy_implementations)]
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
        free(base as *mut c_void);
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
pub fn encode_memory<PixelType: Copy>(image: &[PixelType], w: usize, h: usize, colortype: ColorType, bitdepth: c_uint) -> Result<CVec<u8>, Error> {
    unsafe {
        let mut out = mem::zeroed();
        let mut outsize = 0;

        try!(with_buffer_for_type(image, w, h, colortype, bitdepth, |ptr| {
            ffi::lodepng_encode_memory(&mut out, &mut outsize, ptr, w as c_uint, h as c_uint, colortype, bitdepth)
        }).to_result());
        Ok(cvec_with_free(out, outsize as usize))
    }
}

/// Same as `encode_memory`, but always encodes from 32-bit RGBA raw image
pub fn encode32<PixelType: Copy>(image: &[PixelType], w: usize, h: usize) -> Result<CVec<u8>, Error>  {
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
    save_file(filepath, encoded.as_cslice().as_ref())
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
pub fn convert<PixelType: Copy>(input: &[PixelType], mode_out: &mut ColorMode, mode_in: &ColorMode, w: usize, h: usize, fix_png: bool) -> Result<Image, Error> {
    unsafe {
        let out = mem::zeroed();
        try!(with_buffer_for_type(input, w, h, mode_in.colortype(), mode_in.bitdepth(), |ptr| {
            ffi::lodepng_convert(out, ptr, &mut mode_out.data, &mode_in.data, w as c_uint, h as c_uint, fix_png as c_uint)
        }).to_result());
        Ok(new_bitmap(out, w, h, mode_out.colortype(), mode_out.bitdepth()))
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
#[doc(hidden)]
pub fn auto_choose_color(mode_out: &mut ColorMode, image: *const u8, w: usize, h: usize, mode_in: &ColorMode, auto_convert: AutoConvert) -> Result<(), Error> {
    unsafe {
        ffi::lodepng_auto_choose_color(&mut mode_out.data, image, w as c_uint, h as c_uint, &mode_in.data, auto_convert).to_result()
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
#[doc(hidden)]
pub fn zlib_compress(input: &[u8], settings: &CompressSettings) -> Result<CVec<u8>, Error> {
    unsafe {
        let mut out = mem::zeroed();
        let mut outsize = 0;

        try!(ffi::lodepng_zlib_compress(&mut out, &mut outsize, input.as_ptr(), input.len() as size_t, settings).to_result());
        Ok(cvec_with_free(out, outsize as usize))
    }
}

/// Compress a buffer with deflate. See RFC 1951.
#[doc(hidden)]
pub fn deflate(input: &[u8], settings: &CompressSettings) -> Result<CVec<u8>, Error> {
    unsafe {
        let mut out = mem::zeroed();
        let mut outsize = 0;

        try!(ffi::lodepng_deflate(&mut out, &mut outsize, input.as_ptr(), input.len() as size_t, settings).to_result());
        Ok(cvec_with_free(out, outsize as usize))
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
        ColorMode::new().clone();
        Info::new().clone();
        State::new().clone().info_raw();
    }
}
