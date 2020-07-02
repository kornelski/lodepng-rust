

use crate::rustimpl::lodepng_free;
use crate::rustimpl::lodepng_malloc;

#[allow(non_camel_case_types)]
pub mod ffi;

mod rustimpl;
use crate::rustimpl::*;

mod error;
pub use crate::error::*;
mod huffman;
mod iter;
use crate::iter::*;

pub use rgb::RGB;
pub use rgb::RGBA8 as RGBA;

use std::os::raw::c_uint;
use std::fmt;
use std::mem;
use std::ptr;
use std::slice;
use std::cmp;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::marker::PhantomData;
use std::os::raw::c_void;

pub use crate::ffi::State;
pub use crate::ffi::ColorType;
pub use crate::ffi::CompressSettings;
pub use crate::ffi::DecompressSettings;
pub use crate::ffi::Time;
pub use crate::ffi::DecoderSettings;
pub use crate::ffi::FilterStrategy;
pub use crate::ffi::EncoderSettings;
pub use crate::ffi::Error;

pub use crate::ffi::Info;
pub use crate::ffi::ColorMode;

#[doc(hidden)] #[deprecated(note="use `ColorType::GREY` instead")] pub const LCT_GREY: ColorType = ColorType::GREY;
#[doc(hidden)] #[deprecated(note="use `ColorType::RGB` instead")] pub const LCT_RGB: ColorType = ColorType::RGB;
#[doc(hidden)] #[deprecated(note="use `ColorType::PALETTE` instead")] pub const LCT_PALETTE: ColorType = ColorType::PALETTE;
#[doc(hidden)] #[deprecated(note="use `ColorType::GREY_ALPHA` instead")] pub const LCT_GREY_ALPHA: ColorType = ColorType::GREY_ALPHA;
#[doc(hidden)] #[deprecated(note="use `ColorType::RGBA` instead")] pub const LCT_RGBA: ColorType = ColorType::RGBA;
#[doc(hidden)] #[deprecated(note="use `FilterStrategy::ZERO` instead")] pub const LFS_ZERO: FilterStrategy = FilterStrategy::ZERO;
#[doc(hidden)] #[deprecated(note="use `FilterStrategy::MINSUM` instead")] pub const LFS_MINSUM: FilterStrategy = FilterStrategy::MINSUM;
#[doc(hidden)] #[deprecated(note="use `FilterStrategy::ENTROPY` instead")] pub const LFS_ENTROPY: FilterStrategy = FilterStrategy::ENTROPY;
#[doc(hidden)] #[deprecated(note="use `FilterStrategy::BRUTE_FORCE` instead")] pub const LFS_BRUTE_FORCE: FilterStrategy = FilterStrategy::BRUTE_FORCE;

impl ColorMode {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn colortype(&self) -> ColorType {
        self.colortype
    }

    #[inline]
    pub fn bitdepth(&self) -> u32 {
        self.bitdepth
    }

    pub fn set_bitdepth(&mut self, d: u32) {
        assert!(d >= 1 && d <= 16);
        self.bitdepth = d;
    }

    pub fn palette_clear(&mut self) {
        unsafe {
            lodepng_free(self.palette as *mut _);
        }
        self.palette = ptr::null_mut();
        self.palettesize = 0;
    }

    /// add 1 color to the palette
    pub fn palette_add(&mut self, p: RGBA) -> Result<(), Error> {
        unsafe {
            /*the same resize technique as C++ std::vectors is used, and here it's made so that for a palette with
              the max of 256 colors, it'll have the exact alloc size*/
            if self.palette.is_null() {
                /*allocate palette if empty*/
                /*room for 256 colors with 4 bytes each*/
                self.palette = lodepng_malloc(1024) as *mut _;
                if self.palette.is_null() {
                    return Err(Error(83));
                }
            }
            if self.palettesize >= 256 {
                return Err(Error(38));
            }
            *self.palette.offset(self.palettesize as isize) = p;
            self.palettesize += 1;
        }
        Ok(())
    }

    pub fn palette(&self) -> &[RGBA] {
        unsafe {
            std::slice::from_raw_parts(self.palette, self.palettesize)
        }
    }

    pub fn palette_mut(&mut self) -> &mut [RGBA] {
        unsafe {
            std::slice::from_raw_parts_mut(self.palette as *mut _, self.palettesize)
        }
    }

    /// get the total amount of bits per pixel, based on colortype and bitdepth in the struct
    pub fn bpp(&self) -> u32 {
        lodepng_get_bpp_lct(self.colortype, self.bitdepth()) /*4 or 6*/
    }

    pub(crate) fn clear_key(&mut self) {
        self.key_defined = 0;
    }

    /// `tRNS` chunk
    pub fn set_key(&mut self, r: u16, g: u16, b: u16) {
        self.key_defined = 1;
        self.key_r = c_uint::from(r);
        self.key_g = c_uint::from(g);
        self.key_b = c_uint::from(b);
    }

    pub(crate) fn key(&self) -> Option<(u16, u16, u16)> {
        if self.key_defined != 0 {
            Some((self.key_r as u16, self.key_g as u16, self.key_b as u16))
        } else {
            None
        }
    }

    /// get the amount of color channels used, based on colortype in the struct.
    /// If a palette is used, it counts as 1 channel.
    pub fn channels(&self) -> u8 {
        self.colortype.channels()
    }

    /// is it a greyscale type? (only colortype 0 or 4)
    pub fn is_greyscale_type(&self) -> bool {
        self.colortype == ColorType::GREY || self.colortype == ColorType::GREY_ALPHA
    }

    /// has it got an alpha channel? (only colortype 2 or 6)
    pub fn is_alpha_type(&self) -> bool {
        (self.colortype as u32 & 4) != 0
    }

    /// has it got a palette? (only colortype 3)
    pub fn is_palette_type(&self) -> bool {
        self.colortype == ColorType::PALETTE
    }

    /// only returns true if there is a palette and there is a value in the palette with alpha < 255.
    /// Loops through the palette to check this.
    pub fn has_palette_alpha(&self) -> bool {
        self.palette().iter().any(|p| p.a < 255)
    }

    /// Check if the given color info indicates the possibility of having non-opaque pixels in the PNG image.
    /// Returns true if the image can have translucent or invisible pixels (it still be opaque if it doesn't use such pixels).
    /// Returns false if the image can only have opaque pixels.
    /// In detail, it returns true only if it's a color type with alpha, or has a palette with non-opaque values,
    /// or if "key_defined" is true.
    pub fn can_have_alpha(&self) -> bool {
        self.key().is_some() || self.is_alpha_type() || self.has_palette_alpha()
    }

    /// Returns the byte size of a raw image buffer with given width, height and color mode
    pub fn raw_size(&self, w: u32, h: u32) -> usize {
        /*will not overflow for any color type if roughly w * h < 268435455*/
        let bpp = self.bpp() as usize;
        let n = w as usize * h as usize;
        ((n / 8) * bpp) + ((n & 7) * bpp + 7) / 8
    }

    /*in an idat chunk, each scanline is a multiple of 8 bits, unlike the lodepng output buffer*/
    pub(crate) fn raw_size_idat(&self, w: usize, h: usize) -> usize {
        /*will not overflow for any color type if roughly w * h < 268435455*/
        let bpp = self.bpp() as usize;
        let line = ((w / 8) * bpp) + ((w & 7) * bpp + 7) / 8;
        h * line
    }
}

impl Drop for ColorMode {
    fn drop(&mut self) {
        self.palette_clear()
    }
}

impl Clone for ColorMode {
    fn clone(&self) -> Self {
        let mut c = Self {
            colortype: self.colortype,
            bitdepth: self.bitdepth,
            palette: ptr::null_mut(),
            palettesize: 0,
            key_defined: self.key_defined,
            key_r: self.key_r,
            key_g: self.key_g,
            key_b: self.key_b,
        };
        for &p in self.palette() {
            c.palette_add(p).unwrap();
        }
        c
    }
}

impl Default for ColorMode {
    fn default() -> Self {
        Self {
            key_defined: 0,
            key_r: 0,
            key_g: 0,
            key_b: 0,
            colortype: ColorType::RGBA,
            bitdepth: 8,
            palette: ptr::null_mut(),
            palettesize: 0,
        }
    }
}

impl ColorType {
    /// Create color mode with given type and bitdepth
    pub fn to_color_mode(&self, bitdepth: c_uint) -> ColorMode {
        unsafe {
            ColorMode {
                colortype: *self,
                bitdepth,
                ..mem::zeroed()
            }
        }
    }

    /// channels * bytes per channel = bytes per pixel
    pub fn channels(&self) -> u8 {
        match *self {
            ColorType::GREY | ColorType::PALETTE => 1,
            ColorType::GREY_ALPHA => 2,
            ColorType::BGR |
            ColorType::RGB => 3,
            ColorType::BGRA |
            ColorType::BGRX |
            ColorType::RGBA => 4,
        }
    }
}

impl Time {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Info {
    pub fn new() -> Self {
        Self {
            color: ColorMode::new(),
            interlace_method: 0, compression_method: 0, filter_method: 0,
            background_defined: 0, background_r: 0, background_g: 0, background_b: 0,
            time_defined: 0, time: Time::new(),
            unknown_chunks_data: [ptr::null_mut(), ptr::null_mut(), ptr::null_mut()],
            unknown_chunks_size: [0, 0, 0],
            text_num: 0, text_keys: ptr::null_mut(), text_strings: ptr::null_mut(),
            itext_num: 0, itext_keys: ptr::null_mut(), itext_langtags: ptr::null_mut(),
            itext_transkeys: ptr::null_mut(), itext_strings: ptr::null_mut(),
            phys_defined: 0, phys_x: 0, phys_y: 0, phys_unit: 0,
        }
    }

    pub fn text_keys_cstr(&self) -> TextKeysCStrIter<'_> {
        TextKeysCStrIter {
            k: self.text_keys,
            v: self.text_strings,
            n: self.text_num,
            _p: PhantomData,
        }
    }

    pub fn itext_keys(&self) -> ITextKeysIter<'_> {
        ITextKeysIter {
            k: self.itext_keys,
            l: self.itext_langtags,
            t: self.itext_transkeys,
            s: self.itext_strings,
            n: self.itext_num,
            _p: PhantomData,
        }
    }

    /// use this to clear the texts again after you filled them in
    pub fn clear_text(&mut self) {
        unsafe {
            for i in 0..self.text_num as isize {
                string_cleanup(&mut *self.text_keys.offset(i));
                string_cleanup(&mut *self.text_strings.offset(i));
            }
            lodepng_free(self.text_keys as *mut _);
            lodepng_free(self.text_strings as *mut _);
            self.text_num = 0;
        }
    }

    /// push back both texts at once
    pub fn add_text(&mut self, key: &str, str: &str) -> Result<(), Error> {
        self.push_text(key.as_bytes(), str.as_bytes())
    }

    /// use this to clear the itexts again after you filled them in
    pub fn clear_itext(&mut self) {
        unsafe {
            for i in 0..self.itext_num as isize {
                string_cleanup(&mut *self.itext_keys.offset(i));
                string_cleanup(&mut *self.itext_langtags.offset(i));
                string_cleanup(&mut *self.itext_transkeys.offset(i));
                string_cleanup(&mut *self.itext_strings.offset(i));
            }
            lodepng_free(self.itext_keys as *mut _);
            lodepng_free(self.itext_langtags as *mut _);
            lodepng_free(self.itext_transkeys as *mut _);
            lodepng_free(self.itext_strings as *mut _);
        }
        self.itext_keys = ptr::null_mut();
        self.itext_langtags = ptr::null_mut();
        self.itext_transkeys = ptr::null_mut();
        self.itext_strings = ptr::null_mut();
        self.itext_num = 0;
    }

    /// push back the 4 texts of 1 chunk at once
    pub fn add_itext(&mut self, key: &str, langtag: &str, transkey: &str, text: &str) -> Result<(), Error> {
        self.push_itext(
            key.as_bytes(),
            langtag.as_bytes(),
            transkey.as_bytes(),
            text.as_bytes(),
        )
    }

    pub fn append_chunk(&mut self, position: ChunkPosition, chunk: ChunkRef<'_>) -> Result<(), Error> {
        let set = position as usize;
        unsafe {
            let mut tmp = slice::from_raw_parts_mut(self.unknown_chunks_data[set], self.unknown_chunks_size[set]).to_owned();
            chunk_append(&mut tmp, chunk.data);
            let (data, size) = vec_into_raw(tmp)?;
            self.unknown_chunks_data[set] = data;
            self.unknown_chunks_size[set] = size;
        }
        Ok(())
    }

    pub fn create_chunk<C: AsRef<[u8]>>(&mut self, position: ChunkPosition, chtype: C, data: &[u8]) -> Result<(), Error> {
        let chtype = chtype.as_ref();
        if chtype.len() != 4 {
            return Err(Error(67));
        }
        unsafe {
            ffi::lodepng_chunk_create(
                &mut self.unknown_chunks_data[position as usize],
                &mut self.unknown_chunks_size[position as usize],
                data.len() as c_uint,
                chtype.as_ptr() as *const _,
                data.as_ptr(),
            ).to_result()
        }
    }

    pub fn get<Name: AsRef<[u8]>>(&self, index: Name) -> Option<ChunkRef<'_>> {
        let index = index.as_ref();
        self.unknown_chunks(ChunkPosition::IHDR)
            .chain(self.unknown_chunks(ChunkPosition::PLTE))
            .chain(self.unknown_chunks(ChunkPosition::IDAT))
            .find(|c| c.is_type(index))
    }

    pub fn unknown_chunks(&self, position: ChunkPosition) -> ChunksIter<'_> {
        ChunksIter::new(unsafe {
                slice::from_raw_parts(
                    self.unknown_chunks_data[position as usize],
                    self.unknown_chunks_size[position as usize])
        })
    }


    fn set_unknown_chunks(&mut self, src: &Info) -> Result<(), Error> {
        for i in 0..3 {
            unsafe {
                self.unknown_chunks_size[i] = src.unknown_chunks_size[i];
                self.unknown_chunks_data[i] = lodepng_malloc(src.unknown_chunks_size[i]) as *mut u8;
                if self.unknown_chunks_data[i].is_null() && self.unknown_chunks_size[i] != 0 {
                    return Err(Error(83));
                }
                for j in 0..src.unknown_chunks_size[i] {
                    *self.unknown_chunks_data[i].offset(j as isize) = *src.unknown_chunks_data[i].offset(j as isize);
                }
            }
        }
        Ok(())
    }
}

impl Clone for Info {
    fn clone(&self) -> Self {
        let mut dest = Self {
            compression_method: self.compression_method,
            filter_method: self.filter_method,
            interlace_method: self.interlace_method,
            color: self.color.clone(),
            background_defined: self.background_defined,
            background_r: self.background_r,
            background_g: self.background_g,
            background_b: self.background_b,
            text_num: 0,
            text_keys: ptr::null_mut(),
            text_strings: ptr::null_mut(),
            itext_num: 0,
            itext_keys: ptr::null_mut(),
            itext_langtags: ptr::null_mut(),
            itext_transkeys: ptr::null_mut(),
            itext_strings: ptr::null_mut(),
            time_defined: self.time_defined,
            time: self.time,
            phys_defined: self.phys_defined,
            phys_x: self.phys_x,
            phys_y: self.phys_y,
            phys_unit: self.phys_unit,
            unknown_chunks_data: [ptr::null_mut(), ptr::null_mut(), ptr::null_mut()],
            unknown_chunks_size: [0, 0, 0],
        };
        rustimpl::text_copy(&mut dest, self).unwrap();
        rustimpl::itext_copy(&mut dest, self).unwrap();
        dest.set_unknown_chunks(self).unwrap();
        dest
    }
}

#[derive(Clone, Debug, Default)]
/// Make an image with custom settings
pub struct Encoder {
    state: State,
}

impl Encoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// If true, convert to output format
    #[inline]
    pub fn set_auto_convert(&mut self, mode: bool) {
        self.state.set_auto_convert(mode);
    }

    /// palette_filter_zero controls filtering for low-bitdepth images
    #[inline]
    pub fn set_filter_strategy(&mut self, mode: FilterStrategy, palette_filter_zero: bool) {
        self.state.set_filter_strategy(mode, palette_filter_zero);
    }

    /// gzip text metadata
    #[inline]
    pub fn set_text_compression(&mut self, compr: bool) {
        self.state.encoder.text_compression = if compr {1} else {0};
    }

    /// Compress using another zlib implementation. It's gzip header + deflate + adler32 checksum.
    ///
    /// The callback returns 0 on success.
    /// The callback MUST allocate memory using `libc::malloc`
    #[inline]
    #[allow(deprecated)]
    pub fn set_custom_zlib(&mut self, callback: ffi::custom_compress_callback, context: *const c_void) {
        self.state.encoder.zlibsettings.custom_zlib = callback;
        self.state.encoder.zlibsettings.custom_context = context;
    }

    /// Compress using another deflate implementation. It's just deflate, without headers or checksum.
    ///
    /// The callback returns 0 on success.
    /// The callback MUST allocate memory using `libc::malloc`
    #[inline]
    #[allow(deprecated)]
    pub fn set_custom_deflate(&mut self, callback: ffi::custom_compress_callback, context: *const c_void) {
        self.state.encoder.zlibsettings.custom_deflate = callback;
        self.state.encoder.zlibsettings.custom_context = context;
    }

    #[inline]
    pub fn info_raw(&self) -> &ColorMode {
        self.state.info_raw()
    }

    #[inline]
    /// Color mode of the source bytes to be encoded
    pub fn info_raw_mut(&mut self) -> &mut ColorMode {
        self.state.info_raw_mut()
    }

    #[inline]
    pub fn info_png(&self) -> &Info {
        self.state.info_png()
    }

    #[inline]
    /// Color mode of the file to be created
    pub fn info_png_mut(&mut self) -> &mut Info {
        self.state.info_png_mut()
    }

    #[inline]
    #[allow(deprecated)]
    pub fn encode<PixelType: Copy + 'static>(&mut self, image: &[PixelType], w: usize, h: usize) -> Result<Vec<u8>, Error> {
        self.state.encode(image, w, h)
    }

    #[inline]
    #[allow(deprecated)]
    pub fn encode_file<PixelType: Copy + 'static, P: AsRef<Path>>(&mut self, filepath: P, image: &[PixelType], w: usize, h: usize) -> Result<(), Error> {
        self.state.encode_file(filepath, image, w, h)
    }
}

#[derive(Clone, Debug, Default)]
/// Read an image with custom settings
pub struct Decoder {
    pub(crate) state: State,
}

impl Decoder {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn info_raw(&self) -> &ColorMode {
        self.state.info_raw()
    }

    #[inline]
    /// Preferred color mode for decoding
    pub fn info_raw_mut(&mut self) -> &mut ColorMode {
        self.state.info_raw_mut()
    }

    #[inline]
    /// Actual color mode of the decoded image or inspected file
    pub fn info_png(&self) -> &Info {
        self.state.info_png()
    }

    #[inline]
    pub fn info_png_mut(&mut self) -> &mut Info {
        self.state.info_png_mut()
    }

    /// whether to convert the PNG to the color type you want. Default: yes
    #[inline]
    pub fn color_convert(&mut self, true_or_false: bool) {
        self.state.color_convert(true_or_false);
    }

    /// if false but remember_unknown_chunks is true, they're stored in the unknown chunks.
    #[inline]
    pub fn read_text_chunks(&mut self, true_or_false: bool) {
        self.state.read_text_chunks(true_or_false);
    }

    /// store all bytes from unknown chunks in the `Info` (off by default, useful for a png editor)
    #[inline]
    pub fn remember_unknown_chunks(&mut self, true_or_false: bool) {
        self.state.remember_unknown_chunks(true_or_false);
    }

    /// Decompress ICC profile from iCCP chunk
    #[inline]
    pub fn get_icc(&self) -> Result<Vec<u8>, Error> {
        self.state.get_icc()
    }

    /// Load PNG from buffer using Decoder's settings
    ///
    ///  ```no_run
    ///  # use lodepng::*; let mut state = Decoder::new();
    ///  # let slice = [0u8]; #[allow(unused_variables)] fn do_stuff<T>(_buf: T) {}
    ///
    ///  state.info_raw_mut().colortype = ColorType::RGBA;
    ///  match state.decode(&slice) {
    ///      Ok(Image::RGBA(with_alpha)) => do_stuff(with_alpha),
    ///      _ => panic!("¯\\_(ツ)_/¯")
    ///  }
    ///  ```
    #[inline]
    #[allow(deprecated)]
    pub fn decode<Bytes: AsRef<[u8]>>(&mut self, input: Bytes) -> Result<Image, Error> {
        self.state.decode(input)
    }

    /// Decode a file from disk using Decoder's settings
    #[allow(deprecated)]
    pub fn decode_file<P: AsRef<Path>>(&mut self, filepath: P) -> Result<Image, Error> {
        self.state.decode_file(filepath)
    }

    /// Updates `info_png`. Returns (width, height)
    #[allow(deprecated)]
    #[inline]
    pub fn inspect(&mut self, input: &[u8]) -> Result<(usize, usize), Error> {
        self.state.inspect(input)
    }

    /// use custom zlib decoder instead of built in one
    pub fn set_custom_zlib(&mut self, callback: ffi::custom_decompress_callback, context: *const c_void) {
        self.state.decoder.zlibsettings.custom_zlib = callback;
        self.state.decoder.zlibsettings.custom_context = context;
    }

    /// use custom deflate decoder instead of built in one.
    ///
    /// If custom_zlib is used, custom_inflate is ignored since only the built in zlib function will call custom_inflate
    pub fn set_custom_inflate(&mut self, callback: ffi::custom_decompress_callback, context: *const c_void) {
        self.state.decoder.zlibsettings.custom_inflate = callback;
        self.state.decoder.zlibsettings.custom_context = context;
    }
}

impl State {
    #[deprecated(note = "Use Decoder or Encoder type instead")]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn set_auto_convert(&mut self, mode: bool) {
        self.encoder.auto_convert = mode as c_uint;
    }

    pub fn set_filter_strategy(&mut self, mode: FilterStrategy, palette_filter_zero: bool) {
        self.encoder.filter_strategy = mode;
        self.encoder.filter_palette_zero = if palette_filter_zero {1} else {0};
    }

    /// See `Encoder'
    #[deprecated(note = "Use Encoder type instead of State")]
    pub fn set_custom_zlib(&mut self, callback: ffi::custom_compress_callback, context: *const c_void) {
        self.encoder.zlibsettings.custom_zlib = callback;
        self.encoder.zlibsettings.custom_context = context;
    }

    /// See `Encoder'
    #[deprecated(note = "Use Encoder type instead of State")]
    pub fn set_custom_deflate(&mut self, callback: ffi::custom_compress_callback, context: *const c_void) {
        self.encoder.zlibsettings.custom_deflate = callback;
        self.encoder.zlibsettings.custom_context = context;
    }

    #[inline]
    pub fn info_raw(&self) -> &ColorMode {
        &self.info_raw
    }

    #[inline]
    pub fn info_raw_mut(&mut self) -> &mut ColorMode {
        &mut self.info_raw
    }

    #[inline]
    pub fn info_png(&self) -> &Info {
        &self.info_png
    }

    #[inline]
    pub fn info_png_mut(&mut self) -> &mut Info {
        &mut self.info_png
    }

    /// whether to convert the PNG to the color type you want. Default: yes
    #[inline]
    pub fn color_convert(&mut self, true_or_false: bool) {
        self.decoder.color_convert = if true_or_false { 1 } else { 0 };
    }

    /// if false but remember_unknown_chunks is true, they're stored in the unknown chunks.
    #[inline]
    pub fn read_text_chunks(&mut self, true_or_false: bool) {
        self.decoder.read_text_chunks = if true_or_false { 1 } else { 0 };
    }

    /// store all bytes from unknown chunks in the `Info` (off by default, useful for a png editor)
    #[inline]
    pub fn remember_unknown_chunks(&mut self, true_or_false: bool) {
        self.decoder.remember_unknown_chunks = if true_or_false { 1 } else { 0 };
    }

    /// Decompress ICC profile from iCCP chunk
    pub fn get_icc(&self) -> Result<Vec<u8>, Error> {
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
                return zlib_decompress(&iccp[i+2 ..], &self.decoder.zlibsettings);
            }
        }
        Err(Error(75))
    }

    /// Load PNG from buffer using State's settings
    ///
    ///  ```no_run
    ///  # use lodepng::*; let mut state = State::new();
    ///  # let slice = [0u8]; #[allow(unused_variables)] fn do_stuff<T>(_buf: T) {}
    ///
    ///  state.info_raw_mut().colortype = ColorType::RGBA;
    ///  match state.decode(&slice) {
    ///      Ok(Image::RGBA(with_alpha)) => do_stuff(with_alpha),
    ///      _ => panic!("¯\\_(ツ)_/¯")
    ///  }
    ///  ```
    #[deprecated(note = "Use Decoder type instead of State")]
    pub fn decode<Bytes: AsRef<[u8]>>(&mut self, input: Bytes) -> Result<Image, Error> {
        let input = input.as_ref();
        unsafe {
            let (v, w, h) = rustimpl::lodepng_decode(self, input)?;
            let (data, _) = vec_into_raw(v)?;
            Ok(new_bitmap(data, w, h, self.info_raw.colortype, self.info_raw.bitdepth))
        }
    }

    #[deprecated(note = "Use Decoder type instead of State")]
    #[allow(deprecated)]
    #[inline]
    pub fn decode_file<P: AsRef<Path>>(&mut self, filepath: P) -> Result<Image, Error> {
        self.decode(&std::fs::read(filepath)?)
    }

    /// Updates `info_png`. Returns (width, height)
    #[deprecated(note = "Use Decoder type instead of State")]
    #[inline]
    pub fn inspect(&mut self, input: &[u8]) -> Result<(usize, usize), Error> {
        let (info, w, h) = rustimpl::lodepng_inspect(&self.decoder, input, true)?;
        self.info_png = info;
        Ok((w, h))
    }

    #[deprecated(note = "Use Encoder type instead of State")]
    pub fn encode<PixelType: Copy + 'static>(&mut self, image: &[PixelType], w: usize, h: usize) -> Result<Vec<u8>, Error> {
        let image = buffer_for_type(image, w, h, self.info_raw.colortype, self.info_raw.bitdepth)?;
        Ok(rustimpl::lodepng_encode(image, w as c_uint, h as c_uint, self)?)
    }

    #[deprecated(note = "Use Encoder type instead of State")]
    #[allow(deprecated)]
    #[inline]
    pub fn encode_file<PixelType: Copy + 'static, P: AsRef<Path>>(&mut self, filepath: P, image: &[PixelType], w: usize, h: usize) -> Result<(), Error> {
        let buf = self.encode(image, w, h)?;
        save_file(filepath, buf.as_ref())
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            decoder: DecoderSettings::new(),
            encoder: EncoderSettings::new(),
            info_raw: ColorMode::new(),
            info_png: Info::new(),
            error: Error(1),
        }
    }
}

pub use rgb::alt::Gray as Grey;
pub use rgb::alt::GrayAlpha as GreyAlpha;

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
    RGBA(Bitmap<RGBA>),
    RGB(Bitmap<RGB<u8>>),
    RGBA16(Bitmap<rgb::RGBA<u16>>),
    RGB16(Bitmap<RGB<u16>>),
}

/// Position in the file section after…
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum ChunkPosition {
    IHDR = 0,
    PLTE = 1,
    IDAT = 2,
}

/// Low-level representation of an image
pub struct Bitmap<PixelType: Copy + 'static> {
    /// Raw bitmap memory. Layout depends on color mode and bitdepth used to create it.
    ///
    /// * For RGB/RGBA images one element is one pixel.
    /// * For <8bpp images pixels are packed, so raw bytes are exposed and you need to do bit-twiddling youself
    pub buffer: Vec<PixelType>,
    /// Width in pixels
    pub width: usize,
    /// Height in pixels
    pub height: usize,
}

impl<T: Copy> Bitmap<T> {
    unsafe fn from_buffer(out: *mut u8, w: usize, h: usize) -> Self {
        Self {
            buffer: vec_from_malloced(out as *mut _, w * h),
            width: w,
            height: h,
        }
    }
}

impl<PixelType: Copy + 'static> fmt::Debug for Bitmap<PixelType> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{{} × {} Bitmap}}", self.width, self.height)
    }
}

fn required_size(w: usize, h: usize, colortype: ColorType, bitdepth: u32) -> usize {
    colortype.to_color_mode(bitdepth).raw_size(w as c_uint, h as c_uint)
}

unsafe fn vec_from_malloced<T>(ptr: *mut T, elts: usize) -> Vec<T> where T: Copy {
    let v = Vec::from(std::slice::from_raw_parts(ptr, elts));
    libc::free(ptr as *mut _);
    v
}

unsafe fn new_bitmap(out: *mut u8, w: usize, h: usize, colortype: ColorType, bitdepth: c_uint) -> Image {
    match (colortype, bitdepth) {
        (ColorType::RGBA, 8) => Image::RGBA(Bitmap::from_buffer(out, w, h)),
        (ColorType::RGB, 8) => Image::RGB(Bitmap::from_buffer(out, w, h)),
        (ColorType::RGBA, 16) => Image::RGBA16(Bitmap::from_buffer(out, w, h)),
        (ColorType::RGB, 16) => Image::RGB16(Bitmap::from_buffer(out, w, h)),
        (ColorType::GREY, 8) => Image::Grey(Bitmap::from_buffer(out, w, h)),
        (ColorType::GREY, 16) => Image::Grey16(Bitmap::from_buffer(out, w, h)),
        (ColorType::GREY_ALPHA, 8) => Image::GreyAlpha(Bitmap::from_buffer(out, w, h)),
        (ColorType::GREY_ALPHA, 16) => Image::GreyAlpha16(Bitmap::from_buffer(out, w, h)),
        (_, 0) => panic!("Invalid depth"),
        (c, b) => Image::RawData(Bitmap {
            buffer: vec_from_malloced(out, required_size(w, h, c, b)),
            width: w,
            height: h,
        }),
    }
}

fn save_file<P: AsRef<Path>>(filepath: P, data: &[u8]) -> Result<(), Error> {
    let mut file = File::create(filepath)?;
    file.write_all(data)?;
    Ok(())
}

/// Converts PNG data in memory to raw pixel data.
///
/// `decode32` and `decode24` are more convenient if you want specific image format.
///
/// See `State::decode()` for advanced decoding.
///
/// * `in`: Memory buffer with the PNG file.
/// * `colortype`: the desired color type for the raw output image. See `ColorType`.
/// * `bitdepth`: the desired bit depth for the raw output image. 1, 2, 4, 8 or 16. Typically 8.
pub fn decode_memory<Bytes: AsRef<[u8]>>(input: Bytes, colortype: ColorType, bitdepth: c_uint) -> Result<Image, Error> {
    let input = input.as_ref();
    unsafe {
        assert!(bitdepth > 0 && bitdepth <= 16);
        let (v, w, h) = rustimpl::lodepng_decode_memory(input, colortype, bitdepth)?;
        let (data, _) = vec_into_raw(v)?;
        Ok(new_bitmap(data, w, h, colortype, bitdepth))
    }
}

/// Same as `decode_memory`, but always decodes to 32-bit RGBA raw image
#[inline]
pub fn decode32<Bytes: AsRef<[u8]>>(input: Bytes) -> Result<Bitmap<RGBA>, Error> {
    match decode_memory(input, ColorType::RGBA, 8)? {
        Image::RGBA(img) => Ok(img),
        _ => Err(Error(56)), // given output image colortype or bitdepth not supported for color conversion
    }
}

/// Same as `decode_memory`, but always decodes to 24-bit RGB raw image
#[inline]
pub fn decode24<Bytes: AsRef<[u8]>>(input: Bytes) -> Result<Bitmap<RGB<u8>>, Error> {
    match decode_memory(input, ColorType::RGB, 8)? {
        Image::RGB(img) => Ok(img),
        _ => Err(Error(56)),
    }
}

/// Load PNG from disk, from file with given name.
/// Same as the other decode functions, but instead takes a file path as input.
///
/// `decode32_file` and `decode24_file` are more convenient if you want specific image format.
///
/// There's also `State::decode()` if you'd like to set more settings.
///
///  ```no_run
///  # use lodepng::*; let filepath = std::path::Path::new("");
///  # fn do_stuff<T>(_buf: T) {}
///
///  match decode_file(filepath, ColorType::RGBA, 8) {
///      Ok(Image::RGBA(with_alpha)) => do_stuff(with_alpha),
///      Ok(Image::RGB(without_alpha)) => do_stuff(without_alpha),
///      _ => panic!("¯\\_(ツ)_/¯")
///  }
///  ```
#[inline]
pub fn decode_file<P: AsRef<Path>>(filepath: P, colortype: ColorType, bitdepth: c_uint) -> Result<Image, Error> {
    decode_memory(&std::fs::read(filepath)?, colortype, bitdepth)
}

/// Same as `decode_file`, but always decodes to 32-bit RGBA raw image
#[inline]
pub fn decode32_file<P: AsRef<Path>>(filepath: P) -> Result<Bitmap<RGBA>, Error> {
    match decode_file(filepath, ColorType::RGBA, 8)? {
        Image::RGBA(img) => Ok(img),
        _ => Err(Error(56)),
    }
}

/// Same as `decode_file`, but always decodes to 24-bit RGB raw image
#[inline]
pub fn decode24_file<P: AsRef<Path>>(filepath: P) -> Result<Bitmap<RGB<u8>>, Error> {
    match decode_file(filepath, ColorType::RGB, 8)? {
        Image::RGB(img) => Ok(img),
        _ => Err(Error(56)),
    }
}

fn buffer_for_type<PixelType: Copy + 'static>(image: &[PixelType], w: usize, h: usize, colortype: ColorType, bitdepth: u32) -> Result<&[u8], Error> {
    let bytes_per_pixel = bitdepth as usize/8;
    assert!(mem::size_of::<PixelType>() <= 4*bytes_per_pixel, "Implausibly large {}-byte pixel data type", mem::size_of::<PixelType>());

    let px_bytes = mem::size_of::<PixelType>();
    let image_bytes = image.len() * px_bytes;
    let required_bytes = required_size(w, h, colortype, bitdepth);

    if image_bytes != required_bytes {
        debug_assert_eq!(image_bytes, required_bytes, "Image is {} bytes large ({}x{}x{}), but needs to be {} ({:?}, {})",
            image_bytes, w,h,px_bytes, required_bytes, colortype, bitdepth);
        return Err(Error(84));
    }

    unsafe {
        Ok(slice::from_raw_parts(image.as_ptr() as *const _, image_bytes))
    }
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
pub fn encode_memory<PixelType: Copy + 'static>(image: &[PixelType], w: usize, h: usize, colortype: ColorType, bitdepth: c_uint) -> Result<Vec<u8>, Error> {
    let image = buffer_for_type(image, w, h, colortype, bitdepth)?;
    Ok(rustimpl::lodepng_encode_memory(image, w as u32, h as u32, colortype, bitdepth)?)
}

/// Same as `encode_memory`, but always encodes from 32-bit RGBA raw image
#[inline]
pub fn encode32<PixelType: Copy + 'static>(image: &[PixelType], w: usize, h: usize) -> Result<Vec<u8>, Error> {
    encode_memory(image, w, h, ColorType::RGBA, 8)
}

/// Same as `encode_memory`, but always encodes from 24-bit RGB raw image
#[inline]
pub fn encode24<PixelType: Copy + 'static>(image: &[PixelType], w: usize, h: usize) -> Result<Vec<u8>, Error> {
    encode_memory(image, w, h, ColorType::RGB, 8)
}

/// Converts raw pixel data into a PNG file on disk.
/// Same as the other encode functions, but instead takes a file path as output.
///
/// NOTE: This overwrites existing files without warning!
pub fn encode_file<PixelType: Copy + 'static, P: AsRef<Path>>(filepath: P, image: &[PixelType], w: usize, h: usize, colortype: ColorType, bitdepth: c_uint) -> Result<(), Error> {
    let encoded = encode_memory(image, w, h, colortype, bitdepth)?;
    save_file(filepath, encoded.as_ref())
}

/// Same as `encode_file`, but always encodes from 32-bit RGBA raw image
#[inline]
pub fn encode32_file<PixelType: Copy + 'static, P: AsRef<Path>>(filepath: P, image: &[PixelType], w: usize, h: usize) -> Result<(), Error> {
    encode_file(filepath, image, w, h, ColorType::RGBA, 8)
}

/// Same as `encode_file`, but always encodes from 24-bit RGB raw image
#[inline]
pub fn encode24_file<PixelType: Copy + 'static, P: AsRef<Path>>(filepath: P, image: &[PixelType], w: usize, h: usize) -> Result<(), Error> {
    encode_file(filepath, image, w, h, ColorType::RGB, 8)
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
#[deprecated]
pub fn auto_choose_color(mode_out: &mut ColorMode, image: &[u8], w: usize, h: usize, mode_in: &ColorMode) -> Result<(), Error> {
    *mode_out = rustimpl::auto_choose_color(image, w, h, mode_in)?;
    Ok(())
}

/// Reference to a chunk
#[derive(Copy, Clone)]
pub struct ChunkRef<'a> {
    data: &'a [u8],
}

impl<'a> ChunkRef<'a> {
    #[inline]
    pub(crate) unsafe fn from_ptr(data: *const u8) -> Result<Self, Error> {
        let head = std::slice::from_raw_parts(data, 4);
        let len = lodepng_chunk_length(head);
        let chunk = std::slice::from_raw_parts(data, len + 12);
        Self::new(chunk)
    }

    pub(crate) fn new(data: &'a [u8]) -> Result<Self, Error> {
        if data.len() < 12 {
            return Err(Error(30));
        }
        let len = lodepng_chunk_length(data);
        /*error: chunk length larger than the max PNG chunk size*/
        if len > (1 << 31) {
            return Err(Error(63));
        }
        if data.len() - 12 < len {
            return Err(Error(64));
        }

        Ok(Self {
            data: &data[0..len + 12],
        })
    }

    #[inline]
    pub fn len(&self) -> usize {
        lodepng_chunk_length(self.data)
    }

    /// Chunk type, e.g. `tRNS`
    #[inline]
    pub fn name(&self) -> [u8; 4] {
        let mut tmp = [0; 4];
        tmp.copy_from_slice(&self.data[4..8]);
        tmp
    }

    /// True if `name()` equals this arg
    #[inline]
    pub fn is_type<C: AsRef<[u8]>>(&self, name: C) -> bool {
        self.name() == name.as_ref()
    }

    #[inline]
    pub fn is_ancillary(&self) -> bool {
        (self.data[4] & 32) != 0
    }

    #[inline]
    pub fn is_private(&self) -> bool {
        (self.data[6] & 32) != 0
    }

    #[inline]
    pub fn is_safe_to_copy(&self) -> bool {
        (self.data[7] & 32) != 0
    }

    #[inline]
    pub fn data(&self) -> &[u8] {
        let len = self.len();
        &self.data[8..8 + len]
    }

    #[cfg(not(fuzzing))]
    pub fn check_crc(&self) -> bool {
        let length = self.len();
        /*the CRC is taken of the data and the 4 chunk type letters, not the length*/
        let crc = rustimpl::lodepng_read32bit_int(&self.data[length + 8..]);
        let checksum = self.crc();
        crc == checksum
    }

    #[inline]
    pub fn crc(&self) -> u32 {
        let length = self.len();
        lodepng_crc32(&self.data[4..length + 8])
    }

    #[cfg(fuzzing)]
    /// Disable crc32 checks so that random data from fuzzer gets actually parsed
    #[inline]
    pub fn check_crc(&self) -> bool {
        true
    }

    /// header + data + crc
    #[inline]
    pub(crate) fn whole_chunk_data(&self) -> &[u8] {
        self.data
    }
}

pub struct ChunkRefMut<'a> {
    data: &'a mut [u8],
}

impl<'a> ChunkRefMut<'a> {
    pub fn data_mut(&mut self) -> &mut [u8] {
        let len = ChunkRef::new(self.data).unwrap().len();
        &mut self.data[8..8 + len]
    }

    pub fn generate_crc(&mut self) {
        rustimpl::lodepng_chunk_generate_crc(self.data)
    }
}

/// Compresses data with Zlib.
/// Zlib adds a small header and trailer around the deflate data.
/// The data is output in the format of the zlib specification.
#[doc(hidden)]
pub fn zlib_compress(input: &[u8], settings: &CompressSettings) -> Result<Vec<u8>, Error> {
    let mut v = Vec::with_capacity(input.len()/2);
    rustimpl::lodepng_zlib_compress(&mut v, input, settings)?;
    Ok(v)
}

fn zlib_decompress(input: &[u8], settings: &DecompressSettings) -> Result<Vec<u8>, Error> {
    Ok(rustimpl::lodepng_zlib_decompress(input, settings)?)
}

/// Compress a buffer with deflate. See RFC 1951.
#[doc(hidden)]
pub fn deflate(input: &[u8], settings: &CompressSettings) -> Result<Vec<u8>, Error> {
    Ok(rustimpl::lodepng_deflatev(input, settings)?)
}

impl CompressSettings {
    /// Default compression settings
    pub fn new() -> CompressSettings {
        Self::default()
    }
}

impl Default for CompressSettings {
    fn default() -> Self {
        Self {
            btype: 2,
            use_lz77: true,
            windowsize: DEFAULT_WINDOWSIZE as _,
            minmatch: 3,
            nicematch: 128,
            lazymatching: true,
            custom_zlib: None,
            custom_deflate: None,
            custom_context: ptr::null_mut(),
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
            ignore_adler32: false,
            custom_zlib: None,
            custom_inflate: None,
            custom_context: ptr::null_mut(),
        }
    }
}

impl DecoderSettings {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for DecoderSettings {
    fn default() -> Self {
        Self {
            color_convert: 1,
            read_text_chunks: 1,
            remember_unknown_chunks: 0,
            ignore_crc: 0,
            zlibsettings: DecompressSettings::new(),
        }
    }
}

impl EncoderSettings {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for EncoderSettings {
    fn default() -> Self {
        Self {
            zlibsettings: CompressSettings::new(),
            filter_palette_zero: 1,
            filter_strategy: FilterStrategy::MINSUM,
            auto_convert: 1,
            force_palette: 0,
            predefined_filters: ptr::null_mut(),
            add_id: 0,
            text_compression: 1,
        }
    }
}

#[cfg(test)]
mod test {
    use std::mem;
    use super::*;

    #[test]
    fn pixel_sizes() {
        assert_eq!(4, mem::size_of::<RGBA>());
        assert_eq!(3, mem::size_of::<RGB<u8>>());
        assert_eq!(2, mem::size_of::<GreyAlpha<u8>>());
        assert_eq!(1, mem::size_of::<Grey<u8>>());
    }

    #[test]
    fn create_and_destroy1() {
        DecoderSettings::new();
        EncoderSettings::new();
        CompressSettings::new();
    }

    #[test]
    fn create_and_destroy2() {
        Encoder::new().info_png();
        Encoder::new().info_png_mut();
        Encoder::new().clone().info_raw();
        Encoder::new().clone().info_raw_mut();
    }

    #[test]
    fn test_pal() {
        let mut state = Encoder::new();
        state.info_raw_mut().colortype = ColorType::PALETTE;
        assert_eq!(state.info_raw().colortype(), ColorType::PALETTE);
        state.info_raw_mut().palette_add(RGBA::new(1,2,3,4)).unwrap();
        state.info_raw_mut().palette_add(RGBA::new(5,6,7,255)).unwrap();
        assert_eq!(&[RGBA{r:1u8,g:2,b:3,a:4},RGBA{r:5u8,g:6,b:7,a:255}], state.info_raw().palette());
        state.info_raw_mut().palette_clear();
        assert_eq!(0, state.info_raw().palette().len());
    }

    #[test]
    fn chunks() {
        let mut state = Encoder::new();
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
        let mut dec = Decoder::new();
        dec.remember_unknown_chunks(true);
        dec.decode(img).unwrap();
        let chunk = dec.info_png().unknown_chunks(ChunkPosition::IHDR).next().unwrap();
        assert_eq!("foob".as_bytes(), chunk.name());
        dec.info_png().get("foob").unwrap();
    }

    #[test]
    fn read_icc() {
        let mut s = Decoder::new();
        s.remember_unknown_chunks(true);
        let f = s.decode_file("tests/profile.png");
        f.unwrap();
        let icc = s.info_png().get("iCCP").unwrap();
        assert_eq!(275, icc.len());
        assert_eq!("ICC Pro".as_bytes(), &icc.data()[0..7]);

        let data = s.get_icc().unwrap();
        assert_eq!("appl".as_bytes(), &data[4..8]);
    }

    #[test]
    fn custom_zlib() {
        let mut d = Decoder::new();
        fn custom(inp: &[u8], _: &mut dyn std::io::Write, settings: &DecompressSettings) -> Result<(), Error> {
            assert_eq!(12, inp.len());
            assert_eq!(settings.custom_context, 123 as *const _);
            Err(Error::new(1))
        }
        d.set_custom_zlib(Some(custom), 123 as *const _);
        assert!(d.decode_file("tests/profile.png").is_err());
    }
}
