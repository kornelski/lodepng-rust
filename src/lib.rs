#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::identity_op)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::inline_always)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::new_without_default)]
#![allow(clippy::new_without_default)]
#![allow(clippy::similar_names)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::trivially_copy_pass_by_ref)]
#![allow(clippy::type_complexity)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::verbose_bit_mask)]
#![deny(clippy::unnecessary_mut_passed)]

#[allow(non_camel_case_types)]
pub mod ffi;

mod rustimpl;
mod zlib;

use crate::rustimpl::chunk_length;

mod error;
pub use crate::error::*;
mod iter;
use crate::iter::{ChunksIterFragile, ITextKeysIter, TextKeysIter};

pub use rgb::Pod;
pub use rgb::RGB;
pub use rgb::RGBA8 as RGBA;

use std::cmp;
use std::convert::TryInto;
use std::fmt;
use std::fs;
use std::mem;
use std::num::NonZeroU8;
use std::os::raw::c_uint;
use std::os::raw::c_void;
use std::path::Path;
use std::ptr;
use std::slice;

pub use crate::ffi::ColorType;
pub use crate::ffi::CompressSettings;
pub use crate::ffi::DecoderSettings;
pub use crate::ffi::DecompressSettings;
pub use crate::ffi::EncoderSettings;
pub use crate::ffi::ErrorCode;
pub use crate::ffi::FilterStrategy;
pub use crate::ffi::State;
pub use crate::ffi::Time;

pub use crate::ffi::ColorMode;
pub use crate::ffi::Info;

pub use iter::ChunksIter;

impl ColorMode {
    #[inline(always)]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    #[must_use]
    pub fn colortype(&self) -> ColorType {
        self.colortype
    }

    #[inline(always)]
    pub fn set_colortype(&mut self, color: ColorType) {
        self.colortype = color;
    }

    #[inline(always)]
    #[must_use]
    pub fn bitdepth(&self) -> u32 {
        self.bitdepth
    }

    #[inline]
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn set_bitdepth(&mut self, d: u32) {
        self.try_set_bitdepth(d).unwrap();
    }

    #[inline]
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn try_set_bitdepth(&mut self, d: u32) -> Result<(), Error> {
        if d < 1 || (d > 8 && d != 16) {
            return Err(Error::new(37));
        }
        self.bitdepth = d;
        Ok(())
    }

    /// Set color depth to 8-bit palette and set the colors
    pub fn set_palette(&mut self, palette: &[RGBA]) -> Result<(), Error> {
        self.palette_clear();
        for &c in palette {
            self.palette_add(c)?;
        }
        self.colortype = ColorType::PALETTE;
        self.try_set_bitdepth(8)?;
        Ok(())
    }

    /// Reset to 0 colors
    #[inline]
    pub fn palette_clear(&mut self) {
        self.palette = None;
        self.palettesize = 0;
    }

    /// add 1 color to the palette
    pub fn palette_add(&mut self, p: RGBA) -> Result<(), Error> {
        if self.palettesize >= 256 {
            return Err(Error::new(38));
        }
        let pal = self.palette.get_or_insert_with(|| Box::new([RGBA::new(0,0,0,0); 256]));
        pal[self.palettesize] = p;
        self.palettesize += 1;
        Ok(())
    }

    #[inline]
    #[must_use]
    pub fn palette(&self) -> &[RGBA] {
        let len = self.palettesize;
        self.palette.as_deref().and_then(|p| p.get(..len)).unwrap_or_default()
    }

    #[inline]
    pub fn palette_mut(&mut self) -> &mut [RGBA] {
        let len = self.palettesize;
        self.palette.as_deref_mut().and_then(|p| p.get_mut(..len)).unwrap_or_default()
    }

    /// get the total amount of bits per pixel, based on colortype and bitdepth in the struct
    #[inline(always)]
    #[must_use]
    pub fn bpp(&self) -> u32 {
        self.colortype.bpp(self.bitdepth)
    }

    #[inline(always)]
    #[must_use]
    pub fn bpp_(&self) -> NonZeroU8 {
        self.colortype.bpp_(self.bitdepth as u32)
    }

    pub(crate) fn clear_key(&mut self) {
        self.key_defined = 0;
    }

    /// `tRNS` chunk
    #[cold]
    pub fn set_key(&mut self, r: u16, g: u16, b: u16) {
        self.key_defined = 1;
        self.key_r = c_uint::from(r);
        self.key_g = c_uint::from(g);
        self.key_b = c_uint::from(b);
    }

    #[inline]
    pub(crate) fn key(&self) -> Option<(u16, u16, u16)> {
        if self.key_defined != 0 {
            Some((self.key_r as u16, self.key_g as u16, self.key_b as u16))
        } else {
            None
        }
    }

    /// get the amount of color channels used, based on colortype in the struct.
    /// If a palette is used, it counts as 1 channel.
    #[inline(always)]
    #[must_use]
    pub fn channels(&self) -> u8 {
        self.colortype.channels()
    }

    /// is it a greyscale type? (only colortype 0 or 4)
    #[cfg_attr(docsrs, doc(alias = "is_grayscale_type"))]
    #[cfg_attr(docsrs, doc(alias = "is_gray"))]
    #[inline]
    #[must_use]
    pub fn is_greyscale_type(&self) -> bool {
        self.colortype == ColorType::GREY || self.colortype == ColorType::GREY_ALPHA
    }

    /// has it got an alpha channel? (only colortype 2 or 6)
    #[inline]
    #[must_use]
    pub fn is_alpha_type(&self) -> bool {
        (self.colortype as u32 & 4) != 0
    }

    /// has it got a palette? (only colortype 3)
    #[inline(always)]
    #[must_use]
    pub fn is_palette_type(&self) -> bool {
        self.colortype == ColorType::PALETTE
    }

    /// only returns true if there is a palette and there is a value in the palette with alpha < 255.
    /// Loops through the palette to check this.
    #[must_use]
    pub fn has_palette_alpha(&self) -> bool {
        self.palette().iter().any(|p| p.a < 255)
    }

    /// Check if the given color info indicates the possibility of having non-opaque pixels in the PNG image.
    /// Returns true if the image can have translucent or invisible pixels (it still be opaque if it doesn't use such pixels).
    /// Returns false if the image can only have opaque pixels.
    /// In detail, it returns true only if it's a color type with alpha, or has a palette with non-opaque values,
    /// or if "`key_defined`" is true.
    #[must_use]
    pub fn can_have_alpha(&self) -> bool {
        self.key().is_some() || self.is_alpha_type() || self.has_palette_alpha()
    }

    /// Returns the byte size of a raw image buffer with given width, height and color mode
    #[must_use]
    pub fn raw_size(&self, w: u32, h: u32) -> usize {
        self.raw_size_opt(w as _, h as _).expect("overflow")
    }

    #[inline]
    fn raw_size_opt(&self, w: u32, h: u32) -> Result<usize, Error> {
        let bpp = self.bpp() as usize;
        let n = (w as usize).checked_mul(h as usize).ok_or(Error::new(77))?;
        let res = (n / 8).checked_mul(bpp).ok_or(Error::new(77))?.checked_add(((n & 7) * bpp + 7) / 8).ok_or(Error::new(77))?;
        Ok(res)
    }

    /*in an idat chunk, each scanline is a multiple of 8 bits, unlike the lodepng output buffer*/
    pub(crate) fn raw_size_idat(&self, w: u32, h: u32) -> Option<usize> {
        let w = w as usize;
        let h = h as usize;
        let bpp = self.bpp() as usize;
        let line = (w / 8).checked_mul(bpp)?.checked_add(((w & 7) * bpp + 7) / 8)?;
        h.checked_mul(line)
    }
}

impl Default for ColorMode {
    #[inline]
    fn default() -> Self {
        Self {
            key_defined: 0,
            key_r: 0,
            key_g: 0,
            key_b: 0,
            colortype: ColorType::RGBA,
            bitdepth: 8,
            palette: None,
            palettesize: 0,
        }
    }
}

impl ColorType {
    /// Create color mode with given type and bitdepth
    #[inline]
    #[must_use]
    pub fn to_color_mode(&self, bitdepth: c_uint) -> ColorMode {
        ColorMode {
            colortype: *self,
            bitdepth,
            ..ColorMode::default()
        }
    }

    /// channels * bytes per channel = bytes per pixel
    #[inline]
    #[must_use]
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
    #[inline(always)]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for Info {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Info {
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            color: ColorMode::new(),
            interlace_method: 0,
            background_defined: false, background_r: 0, background_g: 0, background_b: 0,
            time_defined: false, time: Time::new(),
            always_zero_for_ffi_hack: [0, 0, 0],
            unknown_chunks: [Box::new(Vec::new()), Box::new(Vec::new()), Box::new(Vec::new())],
            texts: Vec::new(),
            itexts: Vec::new(),
            phys_defined: false, phys_x: 0, phys_y: 0, phys_unit: 0,
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn text_keys(&self) -> TextKeysIter<'_> {
        TextKeysIter { s: &self.texts }
    }

    #[deprecated(note = "use text_keys")]
    #[must_use]
    pub fn text_keys_cstr(&self) -> TextKeysIter<'_> {
        self.text_keys()
    }

    #[inline(always)]
    #[must_use]
    pub fn itext_keys(&self) -> ITextKeysIter<'_> {
        ITextKeysIter { s: &self.itexts }
    }

    /// use this to clear the texts again after you filled them in
    #[inline]
    pub fn clear_text(&mut self) {
        self.texts = Vec::new();
        self.itexts = Vec::new();
    }

    /// push back both texts at once
    #[inline]
    pub fn add_text(&mut self, key: &str, str: &str) -> Result<(), Error> {
        self.push_text(key.as_bytes(), str.as_bytes())
    }

    /// use this to clear the itexts again after you filled them in
    #[inline]
    pub fn clear_itext(&mut self) {
        self.itexts = Vec::new();
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

    /// Add literal PNG-data chunk unmodified to the unknown chunks
    #[inline]
    pub fn append_chunk(&mut self, position: ChunkPosition, chunk: ChunkRef<'_>) -> Result<(), Error> {
        self.unknown_chunks[position as usize].extend_from_slice(chunk.data);
        Ok(())
    }

    /// Add custom chunk data to the file (if writing one)
    pub fn create_chunk<C: AsRef<[u8]>>(&mut self, position: ChunkPosition, chtype: C, data: &[u8]) -> Result<(), Error> {
        let chtype = chtype.as_ref();
        let chtype = match chtype.try_into() {
            Ok(c) => c,
            Err(_) => return Err(Error::new(67)),
        };
        let mut ch = rustimpl::ChunkBuilder::new(&mut self.unknown_chunks[position as usize], &chtype);
        ch.extend_from_slice(data)?;
        ch.finish()
    }

    /// Uses linear search to find a given chunk. You can use `b"PLTE"` syntax.
    pub fn get<NameBytes: AsRef<[u8]>>(&self, index: NameBytes) -> Option<ChunkRef<'_>> {
        let index = index.as_ref();
        self.try_unknown_chunks(ChunkPosition::IHDR)
            .chain(self.try_unknown_chunks(ChunkPosition::PLTE))
            .chain(self.try_unknown_chunks(ChunkPosition::IDAT))
            .filter_map(|c| c.ok())
            .find(|c| c.is_type(index))
    }

    #[deprecated(note = "use try_unknown_chunks")]
    #[must_use]
    pub fn unknown_chunks(&self, position: ChunkPosition) -> ChunksIterFragile<'_> {
        ChunksIterFragile::new(&self.unknown_chunks[position as usize])
    }

    /// Iterate over chunks that aren't part of image data. Only available if `remember_unknown_chunks` was set.
    #[inline]
    #[must_use]
    pub fn try_unknown_chunks(&self, position: ChunkPosition) -> ChunksIter<'_> {
        ChunksIter::new(&self.unknown_chunks[position as usize])
    }
}

#[derive(Clone, Debug, Default)]
/// Make an image with custom settings
pub struct Encoder {
    state: State,
    predefined_filters: Option<Box<[u8]>>,
}

impl Encoder {
    #[inline(always)]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Switch color mode to 8-bit palette and set the colors
    pub fn set_palette(&mut self, palette: &[RGBA]) -> Result<(), Error> {
        self.state.info_raw_mut().set_palette(palette)?;
        self.state.info_png_mut().color.set_palette(palette)
    }

    /// If true, convert to output format
    #[inline(always)]
    pub fn set_auto_convert(&mut self, mode: bool) {
        self.state.set_auto_convert(mode);
    }

    /// `palette_filter_zero` controls filtering for low-bitdepth images
    #[inline(always)]
    pub fn set_filter_strategy(&mut self, mode: FilterStrategy, palette_filter_zero: bool) {
        if mode != FilterStrategy::PREDEFINED {
            self.predefined_filters = None;
        }
        self.state.set_filter_strategy(mode, palette_filter_zero);
    }

    /// Filters are 0-5, one per row.
    /// <https://www.w3.org/TR/PNG-Filters.html>
    #[inline(always)]
    pub fn set_predefined_filters(&mut self, filters: impl Into<Box<[u8]>>) {
        self.state.set_filter_strategy(FilterStrategy::PREDEFINED, true);
        let filters = filters.into();
        self.settings_mut().predefined_filters = filters.as_ptr();
        self.predefined_filters = Some(filters);
    }

    /// gzip text metadata
    #[inline(always)]
    pub fn set_text_compression(&mut self, compr: bool) {
        self.state.encoder.text_compression = compr;
    }

    /// Compress using another zlib implementation. It's gzip header + deflate + adler32 checksum.
    #[inline(always)]
    #[allow(deprecated)]
    pub fn set_custom_zlib(&mut self, callback: ffi::custom_compress_callback, context: *const c_void) {
        self.state.encoder.zlibsettings.custom_zlib = callback;
        self.state.encoder.zlibsettings.custom_context = context;
    }

    /// Compress using another deflate implementation. It's just deflate, without headers or checksum.
    #[inline(always)]
    #[allow(deprecated)]
    pub fn set_custom_deflate(&mut self, callback: ffi::custom_compress_callback, context: *const c_void) {
        self.state.encoder.zlibsettings.custom_deflate = callback;
        self.state.encoder.zlibsettings.custom_context = context;
    }

    #[inline(always)]
    #[must_use]
    pub fn info_raw(&self) -> &ColorMode {
        self.state.info_raw()
    }

    #[inline(always)]
    /// Color mode of the source bytes to be encoded
    pub fn info_raw_mut(&mut self) -> &mut ColorMode {
        self.state.info_raw_mut()
    }

    #[inline(always)]
    #[must_use]
    pub fn info_png(&self) -> &Info {
        self.state.info_png()
    }

    /// Color mode of the file to be created
    #[inline(always)]
    #[must_use]
    pub fn info_png_mut(&mut self) -> &mut Info {
        self.state.info_png_mut()
    }

    #[inline(always)]
    #[allow(deprecated)]
    #[track_caller]
    /// Takes any pixel type, but for safety the type has to be marked as "plain old data"
    pub fn encode<PixelType: rgb::Pod>(&self, image: &[PixelType], w: usize, h: usize) -> Result<Vec<u8>, Error> {
        if let Some(filters) = &self.predefined_filters {
            if filters.len() < h {
                return Err(Error::new(88))
            }
        }
        self.state.encode(image, w, h)
    }

    #[inline(always)]
    #[allow(deprecated)]
    /// Takes any pixel type, but for safety the type has to be marked as "plain old data"
    pub fn encode_file<PixelType: rgb::Pod, P: AsRef<Path>>(&self, filepath: P, image: &[PixelType], w: usize, h: usize) -> Result<(), Error> {
        if let Some(filters) = &self.predefined_filters {
            if filters.len() < h {
                return Err(Error::new(88));
            }
        }
        self.state.encode_file(filepath, image, w, h)
    }

    #[inline(always)]
    pub fn settings_mut(&mut self) -> &mut EncoderSettings {
        &mut self.state.encoder
    }
}

#[derive(Clone, Debug, Default)]
/// Read an image with custom settings
pub struct Decoder {
    pub(crate) state: State,
}

impl Decoder {
    #[inline(always)]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    #[must_use]
    pub fn info_raw(&self) -> &ColorMode {
        self.state.info_raw()
    }

    #[inline(always)]
    /// Preferred color mode for decoding
    pub fn info_raw_mut(&mut self) -> &mut ColorMode {
        self.state.info_raw_mut()
    }

    #[inline(always)]
    /// Actual color mode of the decoded image or inspected file
    #[must_use]
    pub fn info_png(&self) -> &Info {
        self.state.info_png()
    }

    #[inline(always)]
    pub fn info_png_mut(&mut self) -> &mut Info {
        self.state.info_png_mut()
    }

    /// whether to convert the PNG to the color type you want. Default: yes
    #[inline(always)]
    pub fn color_convert(&mut self, true_or_false: bool) {
        self.state.color_convert(true_or_false);
    }

    /// if false but `remember_unknown_chunks` is true, they're stored in the unknown chunks.
    #[inline(always)]
    pub fn read_text_chunks(&mut self, true_or_false: bool) {
        self.state.read_text_chunks(true_or_false);
    }

    /// store all bytes from unknown chunks in the `Info` (off by default, useful for a png editor)
    #[inline(always)]
    pub fn remember_unknown_chunks(&mut self, true_or_false: bool) {
        self.state.remember_unknown_chunks(true_or_false);
    }

    /// Decompress ICC profile from `iCCP` chunk. Only available if `remember_unknown_chunks` was set.
    #[inline(always)]
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
    #[inline(always)]
    #[allow(deprecated)]
    pub fn decode<Bytes: AsRef<[u8]>>(&mut self, input: Bytes) -> Result<Image, Error> {
        self.state.decode(input)
    }

    /// Decode a file from disk using Decoder's settings
    #[allow(deprecated)]
    #[inline(always)]
    pub fn decode_file<P: AsRef<Path>>(&mut self, filepath: P) -> Result<Image, Error> {
        self.state.decode_file(filepath)
    }

    /// Updates `info_png`. Returns (width, height)
    #[allow(deprecated)]
    #[inline(always)]
    pub fn inspect(&mut self, input: &[u8]) -> Result<(usize, usize), Error> {
        self.state.inspect(input)
    }

    /// use custom zlib decoder instead of built in one
    #[inline(always)]
    pub fn set_custom_zlib(&mut self, callback: ffi::custom_decompress_callback, context: *const c_void) {
        self.state.decoder.zlibsettings.custom_zlib = callback;
        self.state.decoder.zlibsettings.custom_context = context;
    }

    /// use custom deflate decoder instead of built in one.
    ///
    /// If `custom_zlib` is used, `custom_inflate` is ignored since only the built in zlib function will call `custom_inflate`
    #[inline(always)]
    pub fn set_custom_inflate(&mut self, callback: ffi::custom_decompress_callback, context: *const c_void) {
        self.state.decoder.zlibsettings.custom_inflate = callback;
        self.state.decoder.zlibsettings.custom_context = context;
    }
}

impl State {
    #[deprecated(note = "Use Decoder or Encoder type instead")]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    pub fn set_auto_convert(&mut self, mode: bool) {
        self.encoder.auto_convert = mode;
    }

    #[inline(always)]
    pub fn set_filter_strategy(&mut self, mode: FilterStrategy, palette_filter_zero: bool) {
        self.encoder.filter_strategy = mode;
        self.encoder.filter_palette_zero = palette_filter_zero;
    }

    /// See `Encoder`
    #[deprecated(note = "use flate2 crate features instead")]
    #[allow(deprecated)]
    pub fn set_custom_zlib(&mut self, callback: ffi::custom_compress_callback, context: *const c_void) {
        self.encoder.zlibsettings.custom_zlib = callback;
        self.encoder.zlibsettings.custom_context = context;
    }

    /// See `Encoder`
    #[deprecated(note = "use flate2 crate features instead")]
    #[allow(deprecated)]
    pub fn set_custom_deflate(&mut self, callback: ffi::custom_compress_callback, context: *const c_void) {
        self.encoder.zlibsettings.custom_deflate = callback;
        self.encoder.zlibsettings.custom_context = context;
    }

    #[inline(always)]
    #[must_use]
    pub fn info_raw(&self) -> &ColorMode {
        &self.info_raw
    }

    #[inline(always)]
    pub fn info_raw_mut(&mut self) -> &mut ColorMode {
        &mut self.info_raw
    }

    #[inline(always)]
    #[must_use]
    pub fn info_png(&self) -> &Info {
        &self.info_png
    }

    #[inline(always)]
    pub fn info_png_mut(&mut self) -> &mut Info {
        &mut self.info_png
    }

    /// whether to convert the PNG to the color type you want. Default: yes
    #[inline(always)]
    pub fn color_convert(&mut self, true_or_false: bool) {
        self.decoder.color_convert = true_or_false;
    }

    /// if false but `remember_unknown_chunks` is true, they're stored in the unknown chunks.
    #[inline(always)]
    pub fn read_text_chunks(&mut self, true_or_false: bool) {
        self.decoder.read_text_chunks = true_or_false;
    }

    /// store all bytes from unknown chunks in the `Info` (off by default, useful for a png editor)
    #[inline(always)]
    pub fn remember_unknown_chunks(&mut self, true_or_false: bool) {
        self.decoder.remember_unknown_chunks = true_or_false;
    }

    /// Decompress ICC profile from `iCCP` chunk. Only available if `remember_unknown_chunks` was set.
    pub fn get_icc(&self) -> Result<Vec<u8>, Error> {
        let iccp = self.info_png().get("iCCP");
        if iccp.is_none() {
            return Err(Error::new(89));
        }
        let iccp = iccp.as_ref().unwrap().data();
        if iccp.first().copied().unwrap_or(255) == 0 { // text min length is 1
            return Err(Error::new(89));
        }

        let name_len = cmp::min(iccp.len(), 80); // skip name
        for i in 0..name_len {
            if iccp[i] == 0 { // string terminator
                if iccp.get(i+1).copied().unwrap_or(255) != 0 { // compression type
                    return Err(Error::new(72));
                }
                return zlib::decompress(&iccp[i+2 ..], &self.decoder.zlibsettings);
            }
        }
        Err(Error::new(75))
    }

    /// Load PNG from buffer using State's settings
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
    #[deprecated(note = "Use Decoder type instead of State")]
    pub fn decode<Bytes: AsRef<[u8]>>(&mut self, input: Bytes) -> Result<Image, Error> {
        let input = input.as_ref();
        let (data, w, h) = rustimpl::lodepng_decode(self, input)?;
        new_bitmap(data, w, h, self.info_raw.colortype, self.info_raw.bitdepth)
    }

    #[deprecated(note = "Use Decoder type instead of State")]
    #[allow(deprecated)]
    #[inline]
    pub fn decode_file<P: AsRef<Path>>(&mut self, filepath: P) -> Result<Image, Error> {
        self.decode(&fs::read(filepath)?)
    }

    /// Updates `info_png`. Returns (width, height)
    #[deprecated(note = "Use Decoder type instead of State")]
    #[inline]
    pub fn inspect(&mut self, input: &[u8]) -> Result<(usize, usize), Error> {
        let (info, w, h) = rustimpl::lodepng_inspect(&self.decoder, input, true)?;
        self.info_png = info;
        Ok((w as _, h as _))
    }

    #[deprecated(note = "Use Encoder type instead of State")]
    #[track_caller]
    pub fn encode<PixelType: rgb::Pod>(&self, image: &[PixelType], w: usize, h: usize) -> Result<Vec<u8>, Error> {
        let w = w.try_into().map_err(|_| Error::new(93))?;
        let h = h.try_into().map_err(|_| Error::new(93))?;

        let image = buffer_for_type(image, w, h, self.info_raw.colortype, self.info_raw.bitdepth)?;
        rustimpl::lodepng_encode(image, w, h, self)
    }

    #[deprecated(note = "Use Encoder type instead of State")]
    #[allow(deprecated)]
    #[inline]
    #[track_caller]
    pub fn encode_file<PixelType: rgb::Pod, P: AsRef<Path>>(&self, filepath: P, image: &[PixelType], w: usize, h: usize) -> Result<(), Error> {
        let buf = self.encode(image, w, h)?;
        fs::write(filepath, buf)?;
        Ok(())
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            decoder: DecoderSettings::new(),
            encoder: EncoderSettings::new(),
            info_raw: ColorMode::new(),
            info_png: Info::new(),
            error: ErrorCode(1),
        }
    }
}

#[cfg_attr(docsrs, doc(alias = "Gray"))]
pub use rgb::alt::Gray as Grey;
#[cfg_attr(docsrs, doc(alias = "GrayAlpha"))]
pub use rgb::alt::GrayAlpha as GreyAlpha;

/// Bitmap types.
///
/// Images with >=8bpp are stored with pixel per vec element.
/// Images with <8bpp are represented as a bunch of bytes, with multiple pixels per byte.
///
/// Check `decoder.info_raw()` for more info about the image type.
#[derive(Debug)]
pub enum Image {
    /// Bytes of the image. See bpp how many pixels per element there are
    RawData(Bitmap<u8>),
    #[cfg_attr(docsrs, doc(alias = "Gray"))]
    Grey(Bitmap<Grey<u8>>),
    #[cfg_attr(docsrs, doc(alias = "Gray16"))]
    Grey16(Bitmap<Grey<u16>>),
    #[cfg_attr(docsrs, doc(alias = "GrayAlpha"))]
    GreyAlpha(Bitmap<GreyAlpha<u8>>),
    #[cfg_attr(docsrs, doc(alias = "GrayAlpha16"))]
    GreyAlpha16(Bitmap<GreyAlpha<u16>>),
    #[cfg_attr(docsrs, doc(alias = "RGBA8"))]
    RGBA(Bitmap<RGBA>),
    #[cfg_attr(docsrs, doc(alias = "RGB8"))]
    RGB(Bitmap<RGB<u8>>),
    RGBA16(Bitmap<rgb::RGBA<u16>>),
    RGB16(Bitmap<RGB<u16>>),
}

impl Image {
    #[must_use] pub fn width(&self) -> usize {
        match self {
            Self::RawData(bitmap) => bitmap.width,
            Self::Grey(bitmap) => bitmap.width,
            Self::Grey16(bitmap) => bitmap.width,
            Self::GreyAlpha(bitmap) => bitmap.width,
            Self::GreyAlpha16(bitmap) => bitmap.width,
            Self::RGBA(bitmap) => bitmap.width,
            Self::RGB(bitmap) => bitmap.width,
            Self::RGBA16(bitmap) => bitmap.width,
            Self::RGB16(bitmap) => bitmap.width,
        }
    }

    #[must_use] pub fn height(&self) -> usize {
        match self {
            Self::RawData(bitmap) => bitmap.height,
            Self::Grey(bitmap) => bitmap.height,
            Self::Grey16(bitmap) => bitmap.height,
            Self::GreyAlpha(bitmap) => bitmap.height,
            Self::GreyAlpha16(bitmap) => bitmap.height,
            Self::RGBA(bitmap) => bitmap.height,
            Self::RGB(bitmap) => bitmap.height,
            Self::RGBA16(bitmap) => bitmap.height,
            Self::RGB16(bitmap) => bitmap.height,
        }
    }

    /// Raw bytes of the underlying buffer
    #[must_use] pub fn bytes(&self) -> &[u8] {
        use rgb::ComponentBytes;
        match self {
            Self::RawData(bitmap) => {
                let slice = bitmap.buffer.as_slice();
                slice
            },
            Self::Grey(bitmap) => {
                let slice = bitmap.buffer.as_bytes();
                debug_assert_eq!(slice.len(), bitmap.width * bitmap.height);
                slice
            },
            Self::Grey16(bitmap) => {
                let slice = bitmap.buffer.as_bytes();
                debug_assert_eq!(slice.len(), bitmap.width * bitmap.height * 2);
                slice
            },
            Self::GreyAlpha(bitmap) => {
                let slice = bitmap.buffer.as_bytes();
                debug_assert_eq!(slice.len(), bitmap.width * bitmap.height * 2);
                slice
            },
            Self::GreyAlpha16(bitmap) => {
                let slice = bitmap.buffer.as_bytes();
                debug_assert_eq!(slice.len(), bitmap.width * bitmap.height * 4);
                slice
            },
            Self::RGBA(bitmap) => {
                let slice = bitmap.buffer.as_bytes();
                debug_assert_eq!(slice.len(), bitmap.width * bitmap.height * 4);
                slice
            },
            Self::RGB(bitmap) => {
                let slice = bitmap.buffer.as_bytes();
                debug_assert_eq!(slice.len(), bitmap.width * bitmap.height * 3);
                slice
            },
            Self::RGBA16(bitmap) => {
                let slice = bitmap.buffer.as_bytes();
                debug_assert_eq!(slice.len(), bitmap.width * bitmap.height * 8);
                slice
            },
            Self::RGB16(bitmap) => {
                let slice = bitmap.buffer.as_bytes();
                debug_assert_eq!(slice.len(), bitmap.width * bitmap.height * 6);
                slice
            },
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

/// Low-level representation of an image
///
/// Takes any pixel type, but for safety the type has to be marked as "plain old data"
#[derive(Clone)]
pub struct Bitmap<PixelType> {
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

impl<PixelType: rgb::Pod> Bitmap<PixelType> {
    /// Convert Vec<u8> to Vec<PixelType>
    #[cfg_attr(debug_assertions, track_caller)]
    fn from_buffer(buffer: Vec<u8>, width: usize, height: usize) -> Result<Self> {
        let area = width.checked_mul(height).ok_or(Error::new(77))?;

        // Can only cast Vec if alignment doesn't change, and capacity is a round number of pixels
        let is_safe_to_transmute = 1 == std::mem::align_of::<PixelType>() &&
            0 == buffer.capacity() % std::mem::align_of::<PixelType>() &&
            0 == buffer.len() % std::mem::align_of::<PixelType>();

        let buffer = if is_safe_to_transmute {
            unsafe {
                let mut buffer = std::mem::ManuallyDrop::new(buffer);
                Vec::from_raw_parts(buffer.as_mut_ptr().cast::<PixelType>(),
                    buffer.len() / std::mem::size_of::<PixelType>(),
                    buffer.capacity() / std::mem::size_of::<PixelType>())
            }
        } else {
            // if it's not properly aligned (e.g. reading RGB<u16>), do it the hard way
            let mut out = Vec::<PixelType>::new();
            out.try_reserve_exact(area)?;
            let dest = out.spare_capacity_mut();
            assert!(dest.len() >= area);
            unsafe {
                ptr::copy_nonoverlapping(buffer.as_ptr(), dest.as_mut_ptr().cast::<u8>(), buffer.len());
                out.set_len(area);
            }
            out
        };
        if buffer.len() < area {
            return Err(Error::new(84));
        }
        Ok(Self { buffer, width, height })
    }
}

impl<PixelType> fmt::Debug for Bitmap<PixelType> {
    #[cold]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{{} × {} Bitmap}}", self.width, self.height)
    }
}

#[inline]
fn required_size(w: u32, h: u32, colortype: ColorType, bitdepth: u32) -> Result<usize, Error> {
    colortype.to_color_mode(bitdepth).raw_size_opt(w, h)
}

fn new_bitmap(buffer: Vec<u8>, w: u32, h: u32, colortype: ColorType, bitdepth: c_uint) -> Result<Image> {
    debug_assert_eq!(buffer.len(), required_size(w, h, colortype, bitdepth)?);
    let w = w as usize;
    let h = h as usize;

    Ok(match (colortype, bitdepth) {
        (ColorType::RGBA, 8) => Image::RGBA(Bitmap::from_buffer(buffer, w, h)?),
        (ColorType::RGB, 8) => Image::RGB(Bitmap::from_buffer(buffer, w, h)?),
        (ColorType::RGBA, 16) => Image::RGBA16(Bitmap::from_buffer(buffer, w, h)?),
        (ColorType::RGB, 16) => Image::RGB16(Bitmap::from_buffer(buffer, w, h)?),
        (ColorType::GREY, 8) => Image::Grey(Bitmap::from_buffer(buffer, w, h)?),
        (ColorType::GREY, 16) => Image::Grey16(Bitmap::from_buffer(buffer, w, h)?),
        (ColorType::GREY_ALPHA, 8) => Image::GreyAlpha(Bitmap::from_buffer(buffer, w, h)?),
        (ColorType::GREY_ALPHA, 16) => Image::GreyAlpha16(Bitmap::from_buffer(buffer, w, h)?),
        (ColorType::PALETTE | ColorType::GREY, b) if b > 0 && b <= 8 => Image::RawData(Bitmap {
            buffer,
            width: w,
            height: h,
        }),
        // non-standard extension
        (_, b) if b == 8 || b == 16 => Image::RawData(Bitmap {
            buffer,
            width: w,
            height: h,
        }),
        _ => panic!("Invalid depth"),
    })
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
#[inline]
#[cfg_attr(debug_assertions, track_caller)]
pub fn decode_memory<Bytes: AsRef<[u8]>>(input: Bytes, colortype: ColorType, bitdepth: c_uint) -> Result<Image, Error> {
    let input = input.as_ref();
    assert!(bitdepth > 0 && bitdepth <= 16);
    let (data, w, h) = rustimpl::lodepng_decode_memory(input, colortype, bitdepth)?;
    new_bitmap(data, w, h, colortype, bitdepth)
}

/// Same as `decode_memory`, but always decodes to 32-bit RGBA raw image
#[inline]
pub fn decode32<Bytes: AsRef<[u8]>>(input: Bytes) -> Result<Bitmap<RGBA>, Error> {
    match decode_memory(input, ColorType::RGBA, 8)? {
        Image::RGBA(img) => Ok(img),
        _ => Err(Error::new(56)), // given output image colortype or bitdepth not supported for color conversion
    }
}

/// Same as `decode_memory`, but always decodes to 24-bit RGB raw image
#[inline]
pub fn decode24<Bytes: AsRef<[u8]>>(input: Bytes) -> Result<Bitmap<RGB<u8>>, Error> {
    match decode_memory(input, ColorType::RGB, 8)? {
        Image::RGB(img) => Ok(img),
        _ => Err(Error::new(56)),
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
#[inline(always)]
#[cfg_attr(debug_assertions, track_caller)]
pub fn decode_file<P: AsRef<Path>>(filepath: P, colortype: ColorType, bitdepth: c_uint) -> Result<Image, Error> {
    decode_memory(fs::read(filepath)?, colortype, bitdepth)
}

/// Same as `decode_file`, but always decodes to 32-bit RGBA raw image
#[inline]
pub fn decode32_file<P: AsRef<Path>>(filepath: P) -> Result<Bitmap<RGBA>, Error> {
    match decode_file(filepath, ColorType::RGBA, 8)? {
        Image::RGBA(img) => Ok(img),
        _ => Err(Error::new(56)),
    }
}

/// Same as `decode_file`, but always decodes to 24-bit RGB raw image
#[inline]
pub fn decode24_file<P: AsRef<Path>>(filepath: P) -> Result<Bitmap<RGB<u8>>, Error> {
    match decode_file(filepath, ColorType::RGB, 8)? {
        Image::RGB(img) => Ok(img),
        _ => Err(Error::new(56)),
    }
}

#[cfg_attr(debug_assertions, track_caller)]
fn buffer_for_type<PixelType: rgb::Pod>(image: &[PixelType], w: impl TryInto<u32>, h: impl TryInto<u32>, colortype: ColorType, bitdepth: u32) -> Result<&[u8], Error> {
    let w = w.try_into().map_err(|_| Error::new(93))?;
    let h = h.try_into().map_err(|_| Error::new(93))?;

    let required_bytes = required_size(w, h, colortype, bitdepth)?;

    let px_bytes = mem::size_of::<PixelType>();
    let image_bytes = std::mem::size_of_val(image);

    if image_bytes != required_bytes {
        let bpp = colortype.bpp_(bitdepth).get() as usize;
        let ch = colortype.channels();
        assert!(px_bytes == 1 || px_bytes*8 == bpp, "Implausibly large {px_bytes}-byte pixel data type for {bpp}-bit pixels ({ch}ch)");
        debug_assert_eq!(image_bytes, required_bytes, "Image is {image_bytes} bytes large ({w}x{h} {ch}ch), but needs to be {required_bytes}B ({colortype:?}, {bitdepth})");
        return Err(Error::new(84));
    }

    unsafe {
        Ok(slice::from_raw_parts(image.as_ptr().cast::<u8>(), image_bytes))
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
///
/// Takes any pixel type, but for safety the type has to be marked as "plain old data"
#[inline]
#[cfg_attr(debug_assertions, track_caller)]
pub fn encode_memory<PixelType: rgb::Pod>(image: &[PixelType], w: usize, h: usize, colortype: ColorType, bitdepth: c_uint) -> Result<Vec<u8>, Error> {
    let image = buffer_for_type(image, w, h, colortype, bitdepth)?;
    rustimpl::lodepng_encode_memory(image, w as u32, h as u32, colortype, bitdepth)
}

/// Same as `encode_memory`, but always encodes from 32-bit RGBA raw image
#[inline(always)]
#[cfg_attr(debug_assertions, track_caller)]
pub fn encode32<PixelType: rgb::Pod>(image: &[PixelType], w: usize, h: usize) -> Result<Vec<u8>, Error> {
    encode_memory(image, w, h, ColorType::RGBA, 8)
}

/// Same as `encode_memory`, but always encodes from 24-bit RGB raw image
#[inline(always)]
#[cfg_attr(debug_assertions, track_caller)]
pub fn encode24<PixelType: rgb::Pod>(image: &[PixelType], w: usize, h: usize) -> Result<Vec<u8>, Error> {
    encode_memory(image, w, h, ColorType::RGB, 8)
}

/// Converts raw pixel data into a PNG file on disk.
/// Same as the other encode functions, but instead takes a file path as output.
///
/// NOTE: This overwrites existing files without warning!
#[inline]
#[cfg_attr(debug_assertions, track_caller)]
pub fn encode_file<PixelType: rgb::Pod, P: AsRef<Path>>(filepath: P, image: &[PixelType], w: usize, h: usize, colortype: ColorType, bitdepth: c_uint) -> Result<(), Error> {
    let encoded = encode_memory(image, w, h, colortype, bitdepth)?;
    fs::write(filepath, encoded)?;
    Ok(())
}

/// Same as `encode_file`, but always encodes from 32-bit RGBA raw image
#[inline(always)]
#[cfg_attr(debug_assertions, track_caller)]
pub fn encode32_file<PixelType: rgb::Pod, P: AsRef<Path>>(filepath: P, image: &[PixelType], w: usize, h: usize) -> Result<(), Error> {
    encode_file(filepath, image, w, h, ColorType::RGBA, 8)
}

/// Same as `encode_file`, but always encodes from 24-bit RGB raw image
#[inline(always)]
#[cfg_attr(debug_assertions, track_caller)]
pub fn encode24_file<PixelType: rgb::Pod, P: AsRef<Path>>(filepath: P, image: &[PixelType], w: usize, h: usize) -> Result<(), Error> {
    encode_file(filepath, image, w, h, ColorType::RGB, 8)
}

/// Reference to a chunk
#[derive(Copy, Clone)]
pub struct ChunkRef<'a> {
    data: &'a [u8],
}

impl<'a> ChunkRef<'a> {
    #[cfg(feature = "c_ffi")]
    pub(crate) unsafe fn from_ptr(data: *const u8) -> Result<Self, Error> {
        debug_assert!(!data.is_null());
        let head = std::slice::from_raw_parts(data, 4);
        let len = chunk_length(head);
        let chunk = std::slice::from_raw_parts(data, len + 12);
        Self::new(chunk)
    }

    pub(crate) fn new(data: &'a [u8]) -> Result<Self, Error> {
        if data.len() < 12 {
            return Err(Error::new(30));
        }
        let len = chunk_length(data);
        /*error: chunk length larger than the max PNG chunk size*/
        if len > (1 << 31) {
            return Err(Error::new(63));
        }
        if data.len() - 12 < len {
            return Err(Error::new(64));
        }

        Ok(Self {
            data: &data[0..len + 12],
        })
    }

    #[inline(always)]
    #[allow(clippy::len_without_is_empty)]
    #[must_use]
    pub fn len(&self) -> usize {
        chunk_length(self.data)
    }

    /// Chunk type, e.g. `tRNS`
    #[inline]
    #[must_use]
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
    #[must_use]
    pub fn is_ancillary(&self) -> bool {
        (self.data[4] & 32) != 0
    }

    #[inline]
    #[must_use]
    pub fn is_private(&self) -> bool {
        (self.data[6] & 32) != 0
    }

    #[inline]
    #[must_use]
    pub fn is_safe_to_copy(&self) -> bool {
        (self.data[7] & 32) != 0
    }

    #[inline]
    #[must_use]
    pub fn data(&self) -> &'a [u8] {
        let len = self.len();
        &self.data[8..8 + len]
    }

    #[inline]
    #[must_use]
    pub fn crc(&self) -> u32 {
        let length = self.len();
        crc32fast::hash(&self.data[4..length + 8])
    }

    #[cfg(not(fuzzing))]
    #[must_use]
    pub fn check_crc(&self) -> bool {
        let length = self.len();
        /*the CRC is taken of the data and the 4 chunk type letters, not the length*/
        let crc = u32::from_be_bytes(self.data[length + 8.. length + 8 + 4].try_into().unwrap());
        let checksum = self.crc();
        crc == checksum
    }

    #[cfg(fuzzing)]
    /// Disable crc32 checks so that random data from fuzzer gets actually parsed
    #[inline(always)]
    pub fn check_crc(&self) -> bool {
        true
    }

    /// header + data + crc
    #[inline(always)]
    pub(crate) fn whole_chunk_data(&self) -> &[u8] {
        self.data
    }
}

pub struct ChunkRefMut<'a> {
    data: &'a mut [u8],
}

impl<'a> ChunkRefMut<'a> {
    #[inline]
    pub fn data_mut(&mut self) -> &mut [u8] {
        let len = ChunkRef::new(self.data).unwrap().len();
        &mut self.data[8..8 + len]
    }

    #[inline]
    pub fn generate_crc(&mut self) {
        rustimpl::lodepng_chunk_generate_crc(self.data);
    }
}

impl CompressSettings {
    /// Default compression settings
    #[inline(always)]
    #[must_use]
    pub fn new() -> CompressSettings {
        Self::default()
    }
}

impl Default for CompressSettings {
    #[inline]
    #[allow(deprecated)]
    fn default() -> Self {
        Self {
            btype: 0,
            use_lz77: true,
            windowsize: 0,
            minmatch: 0,
            nicematch: 0,
            lazymatching: false,
            custom_zlib: None,
            custom_deflate: None,
            custom_context: ptr::null_mut(),
        }
    }
}

impl DecompressSettings {
    #[inline(always)]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for DecompressSettings {
    #[inline]
    fn default() -> Self {
        Self {
            custom_zlib: None,
            custom_inflate: None,
            custom_context: ptr::null_mut(),
        }
    }
}

impl DecoderSettings {
    #[inline(always)]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for DecoderSettings {
    #[inline]
    fn default() -> Self {
        Self {
            color_convert: true,
            read_text_chunks: true,
            remember_unknown_chunks: false,
            ignore_crc: false,
            zlibsettings: DecompressSettings::new(),
        }
    }
}

impl EncoderSettings {
    #[inline(always)]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Zlib compression level 0=none, 9=best
    pub fn set_level(&mut self, level: u8) {
        self.zlibsettings.set_level(level);
    }
}

impl Default for EncoderSettings {
    #[inline]
    fn default() -> Self {
        Self {
            zlibsettings: CompressSettings::new(),
            filter_palette_zero: true,
            filter_strategy: FilterStrategy::MINSUM,
            auto_convert: true,
            force_palette: false,
            predefined_filters: ptr::null_mut(),
            add_id: false,
            text_compression: true,
        }
    }
}

#[inline]
fn zero_vec(size: usize) -> Result<Vec<u8>, Error> {
    let mut vec = Vec::new(); vec.try_reserve_exact(size)?;
    vec.resize(size, 0);
    Ok(vec)
}

#[cfg(test)]
mod test {
    use super::*;
    use std::mem;

    #[test]
    fn pixel_sizes() {
        assert_eq!(4, mem::size_of::<RGBA>());
        assert_eq!(3, mem::size_of::<RGB<u8>>());
        assert_eq!(2, mem::size_of::<GreyAlpha<u8>>());
        assert_eq!(1, mem::size_of::<Grey<u8>>());

        encode_memory(&[0u8, 1], 4, 4, crate::ColorType::GREY, 1).unwrap();
        encode_memory(&[0u8, 1], 3, 2, crate::ColorType::GREY, 2).unwrap();
        encode_memory(&[0u8, 1], 1, 4, crate::ColorType::GREY, 4).unwrap();
        encode_memory(&[0u8, 1], 2, 1, crate::ColorType::GREY, 8).unwrap();
    }

    #[test]
    fn create_and_destroy1() {
        let _ = DecoderSettings::new();
        let _ = EncoderSettings::new();
        let _ = CompressSettings::new();
    }

    #[test]
    fn create_and_destroy2() {
        let _ = Encoder::new().info_png();
        let _ = Encoder::new().info_png_mut();
        let _ = Encoder::new().info_raw();
        let _ = Encoder::new().info_raw_mut();
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
            for _ in info.try_unknown_chunks(ChunkPosition::IHDR) {
                panic!("no chunks yet");
            }

            let testdata = &[1, 2, 3];
            info.create_chunk(ChunkPosition::PLTE, [255, 0, 100, 32], testdata).unwrap();
            assert_eq!(1, info.try_unknown_chunks(ChunkPosition::PLTE).count());

            info.create_chunk(ChunkPosition::IHDR, "foob", testdata).unwrap();
            assert_eq!(1, info.try_unknown_chunks(ChunkPosition::IHDR).count());
            info.create_chunk(ChunkPosition::IHDR, "foob", testdata).unwrap();
            assert_eq!(2, info.try_unknown_chunks(ChunkPosition::IHDR).count());

            for _ in info.try_unknown_chunks(ChunkPosition::IDAT) {}
            let chunk = info.try_unknown_chunks(ChunkPosition::IHDR).next().unwrap().unwrap();
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
        let chunk = dec.info_png().try_unknown_chunks(ChunkPosition::IHDR).next().unwrap().unwrap();
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
