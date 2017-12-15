#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
#![cfg_attr(feature = "cargo-clippy", allow(identity_op))]
#![cfg_attr(feature = "cargo-clippy", allow(cast_lossless))]
#![cfg_attr(feature = "cargo-clippy", allow(unreadable_literal))]
#![cfg_attr(feature = "cargo-clippy", allow(doc_markdown))]
#![cfg_attr(feature = "cargo-clippy", allow(new_without_default_derive))]
#![cfg_attr(feature = "cargo-clippy", allow(new_without_default))]
#![cfg_attr(feature = "cargo-clippy", allow(verbose_bit_mask))]
#![cfg_attr(feature = "cargo-clippy", allow(many_single_char_names))]
#![cfg_attr(feature = "cargo-clippy", allow(toplevel_ref_arg))]
#![cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
#![cfg_attr(feature = "cargo-clippy", allow(if_same_then_else))]
#![cfg_attr(feature = "cargo-clippy", allow(too_many_arguments))]
#![cfg_attr(feature = "cargo-clippy", allow(cyclomatic_complexity))]

extern crate libc;

use super::*;
use ffi::ColorProfile;
use ffi::State;
use huffman::HuffmanTree;

pub use ucvec::*;
pub use rgb::RGBA8 as RGBA;
use std::collections::HashMap;
use std::ffi::CStr;
use std::fs;
use std::io::prelude::*;
use std::mem;
use std::num::Wrapping;
use std::os::raw::*;
use std::path::*;
use std::ptr;
use std::slice;

pub(crate) fn lodepng_malloc(size: usize) -> *mut c_void {
    unsafe {
        libc::malloc(size) as *mut _
    }
}

pub(crate) unsafe fn lodepng_realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
    libc::realloc(ptr as *mut _, size) as *mut _
}

pub(crate) unsafe fn lodepng_free(ptr: *mut c_void) {
    libc::free(ptr as *mut _)
}

/*8 bytes PNG signature, aka the magic bytes*/
fn writeSignature(out: &mut ucvector) {
    out.push(137u8); /*width*/
    out.push(80u8); /*height*/
    out.push(78u8); /*bit depth*/
    out.push(71u8); /*color type*/
    out.push(13u8); /*compression method*/
    out.push(10u8); /*filter method*/
    out.push(26u8); /*interlace method*/
    out.push(10u8); /*add all channels except alpha channel*/
}

#[derive(Eq, PartialEq)]
enum PaletteTranslucency {
    Opaque,
    Key,
    Semi,
}

/*
palette must have 4 * palettesize bytes allocated, and given in format RGBARGBARGBARGBA...
returns 0 if the palette is opaque,
returns 1 if the palette has a single color with alpha 0 ==> color key
returns 2 if the palette is semi-translucent.
*/
fn getPaletteTranslucency(palette: &[RGBA]) -> PaletteTranslucency {
    let mut key = PaletteTranslucency::Opaque;
    let mut r = 0;
    let mut g = 0;
    let mut b = 0;
    /*the value of the color with alpha 0, so long as color keying is possible*/
    let mut i = 0;
    while i < palette.len() {
        if key == PaletteTranslucency::Opaque && palette[i].a == 0 {
            r = palette[i].r;
            g = palette[i].g;
            b = palette[i].b;
            key = PaletteTranslucency::Key;
            i = 0;
            /*restart from beginning, to detect earlier opaque colors with key's value*/
            continue;
        } else if palette[i].a != 255 {
            return PaletteTranslucency::Semi;
        } else if key == PaletteTranslucency::Key && r == palette[i].r && g == palette[i].g && b == palette[i].b {
            /*when key, no opaque RGB may have key's RGB*/
            return PaletteTranslucency::Semi;
        }
        i += 1;
    }
    key
}

/*The opposite of the removePaddingBits function
  olinebits must be >= ilinebits*/
fn addPaddingBits(out: &mut [u8], inp: &[u8], olinebits: usize, ilinebits: usize, h: usize) {
    let diff = olinebits - ilinebits; /*bit pointers*/
    let mut obp = 0;
    let mut ibp = 0;
    for _ in 0..h {
        for _ in 0..ilinebits {
            let bit = readBitFromReversedStream(&mut ibp, inp);
            setBitOfReversedStream(&mut obp, out, bit);
        }
        for _ in 0..diff {
            setBitOfReversedStream(&mut obp, out, 0u8);
        }
    }
}

/*out must be buffer big enough to contain uncompressed IDAT chunk data, and in must contain the full image.
return value is error**/
fn preProcessScanlines(inp: &[u8], w: usize, h: usize, info_png: &Info, settings: &EncoderSettings) -> Result<Vec<u8>, Error> {
    let h = h as usize;
    let w = w as usize;
    /*
      This function converts the pure 2D image with the PNG's colortype, into filtered-padded-interlaced data. Steps:
      *) if no Adam7: 1) add padding bits (= posible extra bits per scanline if bpp < 8) 2) filter
      *) if adam7: 1) Adam7_interlace 2) 7x add padding bits 3) 7x filter
      */
    let bpp = info_png.color.bpp() as usize;
    if info_png.interlace_method == 0 {
        let outsize = h + (h * ((w * bpp + 7) / 8));
        let mut out = vec![0u8; outsize];
        /*image size plus an extra byte per scanline + possible padding bits*/
        if bpp < 8 && w * bpp != ((w * bpp + 7) / 8) * 8 {
            let mut padded = vec![0u8; (h * ((w * bpp + 7) / 8))]; /*we can immediately filter into the out buffer, no other steps needed*/
            addPaddingBits(&mut padded, inp, ((w * bpp + 7) / 8) * 8, w * bpp, h);
            filter(&mut out, &padded, w, h, &info_png.color, settings)?;
        } else {
            filter(&mut out, inp, w, h, &info_png.color, settings)?;
        }
        Ok(out)
    } else {
        let (passw, passh, filter_passstart, padded_passstart, passstart) = Adam7_getpassvalues(w, h, bpp);
        let outsize = filter_passstart[7];
        /*image size plus an extra byte per scanline + possible padding bits*/
        let mut out = vec![0u8; outsize];
        let mut adam7 = vec![0u8; passstart[7] + 1];
        Adam7_interlace(&mut adam7, inp, w, h, bpp);
        for i in 0..7 {
            if bpp < 8 {
                let mut padded = vec![0u8; (padded_passstart[i + 1] - padded_passstart[i])];
                addPaddingBits(
                    &mut padded,
                    &adam7[passstart[i]..],
                    ((passw[i] as usize * bpp + 7) / 8) * 8,
                    passw[i] as usize * bpp,
                    passh[i] as usize,
                );
                filter(&mut out[filter_passstart[i]..], &padded, passw[i] as usize, passh[i] as usize, &info_png.color, settings)?;
            } else {
                filter(
                    &mut out[filter_passstart[i]..],
                    &adam7[padded_passstart[i]..],
                    passw[i] as usize,
                    passh[i] as usize,
                    &info_png.color,
                    settings,
                )?;
            }
        }
        Ok(out)
    }
}

/*
  For PNG filter method 0
  out must be a buffer with as size: h + (w * h * bpp + 7) / 8, because there are
  the scanlines with 1 extra byte per scanline
  */
fn filter(out: &mut [u8], inp: &[u8], w: usize, h: usize, info: &ColorMode, settings: &EncoderSettings) -> Result<(), Error> {
    let bpp = info.bpp() as usize;

    /*the width of a scanline in bytes, not including the filter type*/
    let linebytes = ((w * bpp + 7) / 8) as usize;
    /*bytewidth is used for filtering, is 1 when bpp < 8, number of bytes per pixel otherwise*/
    let bytewidth = (bpp + 7) / 8;
    let mut prevline = None;
    /*
      There is a heuristic called the minimum sum of absolute differences heuristic, suggested by the PNG standard:
       *  If the image type is Palette, or the bit depth is smaller than 8, then do not filter the image (i.e.
          use fixed filtering, with the filter None).
       * (The other case) If the image type is Grayscale or RGB (with or without Alpha), and the bit depth is
         not smaller than 8, then use adaptive filtering heuristic as follows: independently for each row, apply
         all five filters and select the filter that produces the smallest sum of absolute values per row.
      This heuristic is used if filter strategy is FilterStrategy::MINSUM and filter_palette_zero is true.

      If filter_palette_zero is true and filter_strategy is not FilterStrategy::MINSUM, the above heuristic is followed,
      but for "the other case", whatever strategy filter_strategy is set to instead of the minimum sum
      heuristic is used.
      */
    let strategy = if settings.filter_palette_zero != 0 && (info.colortype == ColorType::PALETTE || info.bitdepth() < 8) {
        FilterStrategy::ZERO
    } else {
        settings.filter_strategy
    };
    if bpp == 0 {
        return Err(Error(31));
    }
    match strategy {
        FilterStrategy::ZERO => for y in 0..h {
            let outindex = (1 + linebytes) * y;
            let inindex = linebytes * y;
            out[outindex] = 0u8;
            filterScanline(&mut out[(outindex + 1)..], &inp[inindex..], prevline, linebytes, bytewidth, 0u8);
            prevline = Some(&inp[inindex..]);
        },
        FilterStrategy::MINSUM => {
            let mut sum: [usize; 5] = [0, 0, 0, 0, 0];
            let mut attempt = [
                vec![0u8; linebytes],
                vec![0u8; linebytes],
                vec![0u8; linebytes],
                vec![0u8; linebytes],
                vec![0u8; linebytes],
            ];
            let mut smallest = 0;
            let mut bestType = 0;
            for y in 0..h {
                for type_ in 0..5 {
                    filterScanline(&mut attempt[type_], &inp[(y * linebytes)..], prevline, linebytes, bytewidth, type_ as u8);
                    sum[type_] = if type_ == 0 {
                        attempt[type_][0..linebytes].iter().map(|&s| s as usize).sum()
                    } else {
                        /*For differences, each byte should be treated as signed, values above 127 are negative
                          (converted to signed char). Filtertype 0 isn't a difference though, so use unsigned there.
                          This means filtertype 0 is almost never chosen, but that is justified.*/
                        attempt[type_][0..linebytes].iter().map(|&s| if s < 128 { s } else { 255 - s } as usize).sum()
                    };
                    /*check if this is smallest sum (or if type == 0 it's the first case so always store the values)*/
                    if type_ == 0 || sum[type_] < smallest {
                        bestType = type_; /*now fill the out values*/
                        smallest = sum[type_];
                    };
                }
                prevline = Some(&inp[(y * linebytes)..]);
                out[y * (linebytes + 1)] = bestType as u8;
                /*the first byte of a scanline will be the filter type*/
                for x in 0..linebytes {
                    out[y * (linebytes + 1) + 1 + x] = attempt[bestType][x];
                } /*try the 5 filter types*/
            } /*the filter type itself is part of the scanline*/
        },
        FilterStrategy::ENTROPY => {
            let mut sum: [f32; 5] = [0., 0., 0., 0., 0.];
            let mut smallest = 0.;
            let mut bestType = 0;
            let mut attempt = [
                vec![0u8; linebytes],
                vec![0u8; linebytes],
                vec![0u8; linebytes],
                vec![0u8; linebytes],
                vec![0u8; linebytes],
            ];
            for y in 0..h {
                for type_ in 0..5 {
                    filterScanline(&mut attempt[type_], &inp[(y * linebytes)..], prevline, linebytes, bytewidth, type_ as u8);
                    let mut count: [u32; 256] = [0; 256];
                    for x in 0..linebytes {
                        count[attempt[type_][x] as usize] += 1;
                    }
                    count[type_] += 1;
                    sum[type_] = 0.;
                    for &c in count.iter() {
                        let p = c as f32 / ((linebytes + 1) as f32);
                        sum[type_] += if c == 0 { 0. } else { (1. / p).log2() * p };
                    }
                    /*check if this is smallest sum (or if type == 0 it's the first case so always store the values)*/
                    if type_ == 0 || sum[type_] < smallest {
                        bestType = type_; /*now fill the out values*/
                        smallest = sum[type_]; /*the first byte of a scanline will be the filter type*/
                    }; /*the extra filterbyte added to each row*/
                }
                prevline = Some(&inp[(y * linebytes)..]);
                out[y * (linebytes + 1)] = bestType as u8;
                for x in 0..linebytes {
                    out[y * (linebytes + 1) + 1 + x] = attempt[bestType][x];
                }
            }
        },
        FilterStrategy::PREDEFINED => for y in 0..h {
            let outindex = (1 + linebytes) * y;
            let inindex = linebytes * y;
            let filters = unsafe { settings.predefined_filters(h)? };
            let type_ = filters[y];
            out[outindex] = type_;
            filterScanline(&mut out[(outindex + 1)..], &inp[inindex..], prevline, linebytes, bytewidth, type_);
            prevline = Some(&inp[inindex..]);
        },
        FilterStrategy::BRUTE_FORCE => {
            /*brute force filter chooser.
            deflate the scanline after every filter attempt to see which one deflates best.
            This is very slow and gives only slightly smaller, sometimes even larger, result*/
            let mut size: [usize; 5] = [0, 0, 0, 0, 0]; /*five filtering attempts, one for each filter type*/
            let mut smallest = 0;
            let mut bestType = 0;
            let mut zlibsettings = settings.zlibsettings.clone();
            /*use fixed tree on the attempts so that the tree is not adapted to the filtertype on purpose,
            to simulate the true case where the tree is the same for the whole image. Sometimes it gives
            better result with dynamic tree anyway. Using the fixed tree sometimes gives worse, but in rare
            cases better compression. It does make this a bit less slow, so it's worth doing this.*/
            zlibsettings.btype = 1;
            /*a custom encoder likely doesn't read the btype setting and is optimized for complete PNG
            images only, so disable it*/
            zlibsettings.custom_zlib = None;
            zlibsettings.custom_deflate = None; /*try the 5 filter types*/
            let mut attempt = [
                vec![0u8; linebytes],
                vec![0u8; linebytes],
                vec![0u8; linebytes],
                vec![0u8; linebytes],
                vec![0u8; linebytes],
            ];
            for y in 0..h {
                for type_ in 0..5 {
                    /*it already works good enough by testing a part of the row*/
                    filterScanline(&mut attempt[type_], &inp[(y * linebytes)..], prevline, linebytes, bytewidth, type_ as u8);
                    size[type_] = 0;
                    let _ = zlib_compress(&attempt[type_], &zlibsettings)?;
                    /*check if this is smallest size (or if type == 0 it's the first case so always store the values)*/
                    if type_ == 0 || size[type_] < smallest {
                        bestType = type_; /*the first byte of a scanline will be the filter type*/
                        smallest = size[type_]; /* unknown filter strategy */
                    }
                }
                prevline = Some(&inp[(y * linebytes)..]);
                out[y * (linebytes + 1)] = bestType as u8;
                for x in 0..linebytes {
                    out[y * (linebytes + 1) + 1 + x] = attempt[bestType][x];
                }
            }
        },
    };
    Ok(())
}

#[test]
fn test_filter() {
    let mut line1 = Vec::with_capacity(1<<16);
    let mut line2 = Vec::with_capacity(1<<16);
    for p in 0..256 {
        for q in 0..256 {
            line1.push(q as u8);
            line2.push(p as u8);
        }
    }

    let mut filtered = vec![99u8; 1<<16];
    let mut unfiltered = vec![66u8; 1<<16];
    for filterType in 0..5 {
        let len = filtered.len();
        filterScanline(&mut filtered, &line1, Some(&line2), len, 1, filterType);
        unfilterScanline(&mut unfiltered, &filtered, Some(&line2), 1, filterType, len).unwrap();
        assert_eq!(unfiltered, line1, "prev+filter={}", filterType);
    }
    for filterType in 0..5 {
        let len = filtered.len();
        filterScanline(&mut filtered, &line1, None, len, 1, filterType);
        unfilterScanline(&mut unfiltered, &filtered, None, 1, filterType, len).unwrap();
        assert_eq!(unfiltered, line1, "none+filter={}", filterType);
    }
}

fn filterScanline(out: &mut [u8], scanline: &[u8], prevline: Option<&[u8]>, length: usize, bytewidth: usize, filterType: u8) {
    let out = unsafe { &mut *(out as *mut [u8] as *mut [Wrapping<u8>]) };
    let scanline = unsafe { &*(scanline as *const [u8] as *const [Wrapping<u8>]) };
    let prevline = prevline.map(|p| unsafe { &*(p as *const [u8] as *const [Wrapping<u8>]) });

    match filterType {
        0 => {
            out[..length].clone_from_slice(&scanline[..length]);
        },
        1 => {
            out[..bytewidth].clone_from_slice(&scanline[..bytewidth]);
            for i in bytewidth..length {
                out[i] = scanline[i] - scanline[i - bytewidth];
            }
        },
        2 => if let Some(prevline) = prevline {
            for i in 0..length {
                out[i] = scanline[i] - prevline[i];
            }
        } else {
            out[..length].clone_from_slice(&scanline[..length]);
        },
        3 => if let Some(prevline) = prevline {
            for i in 0..bytewidth {
                out[i] = scanline[i] - (prevline[i] >> 1);
            }
            for i in bytewidth..length {
                out[i] = scanline[i] - ((scanline[i - bytewidth] + prevline[i]) >> 1);
            }
        } else {
            out[..bytewidth].clone_from_slice(&scanline[..bytewidth]);
            for i in bytewidth..length {
                out[i] = scanline[i] - (scanline[i - bytewidth] >> 1);
            }
        },
        4 => if let Some(prevline) = prevline {
            for i in 0..bytewidth {
                out[i] = scanline[i] - prevline[i];
            }
            for i in bytewidth..length {
                out[i] = scanline[i] - Wrapping(paethPredictor(scanline[i - bytewidth].0.into(), prevline[i].0.into(), prevline[i - bytewidth].0.into()));
            }
        } else {
            out[..bytewidth].clone_from_slice(&scanline[..bytewidth]);
            for i in bytewidth..length {
                out[i] = scanline[i] - scanline[i - bytewidth];
            }
        },
        _ => return,
    };
}

fn paethPredictor(a: i16, b: i16, c: i16) -> u8 {
    let pa = (b - c).abs();
    let pb = (a - c).abs();
    let pc = (a + b - c - c).abs();
    if pc < pa && pc < pb {
        c as u8
    } else if pb < pa {
        b as u8
    } else {
        a as u8
    }
}

pub fn lodepng_encode_file(filename: &Path, image: &[u8], w: u32, h: u32, colortype: ColorType, bitdepth: u32) -> Result<(), Error> {
    let v = lodepng_encode_memory(image, w, h, colortype, bitdepth)?;
    lodepng_save_file(v.slice(), filename)
}

pub(crate) fn lodepng_get_bpp_lct(colortype: ColorType, bitdepth: u32) -> u32 {
    assert!(bitdepth >= 1 && bitdepth <= 16);
    /*bits per pixel is amount of channels * bits per channel*/
    let ch = colortype.channels() as u32;
    return ch * if ch > 1 {
        if bitdepth == 8 {
            8
        } else {
            16
        }
    } else {
        bitdepth
    };
}

pub fn lodepng_get_raw_size_lct(w: u32, h: u32, colortype: ColorType, bitdepth: u32) -> usize {
    /*will not overflow for any color type if roughly w * h < 268435455*/
    let bpp = lodepng_get_bpp_lct(colortype, bitdepth) as usize;
    let n = w as usize * h as usize;
    ((n / 8) * bpp) + ((n & 7) * bpp + 7) / 8
}

/* ////////////////////////////////////////////////////////////////////////// */
/* / Deflate - Huffman                                                      / */
/* ////////////////////////////////////////////////////////////////////////// */
/*256 literals, the end code, some length codes, and 2 unused codes*/
/*the distance codes have their own symbols, 30 used, 2 unused*/
/*the code length codes. 0-15: code lengths, 16: copy previous 3-6 times, 17: 3-10 zeros, 18: 11-138 zeros*/
/*the base lengths represented by codes 257-285*/
pub const LENGTHBASE: [u32; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258, ];
/*the extra bits used by codes 257-285 (added to base length)*/
pub const LENGTHEXTRA: [u32; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0, ];
/*the base backwards distances (the bits of distance codes appear after length codes and use their own huffman tree)*/
pub const DISTANCEBASE: [u32; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577, ];
/*the extra bits of backwards distances (added to base)*/
pub const DISTANCEEXTRA: [u32; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13, ];

pub(crate) unsafe fn string_cleanup(out: &mut *mut i8) {
    lodepng_free((*out) as *mut _);
    *out = ptr::null_mut();
}

pub(crate) fn string_copy(inp: &CStr) -> *mut i8 {
    string_copy_slice(inp.to_bytes())
}

pub(crate) fn string_copy_slice(inp: &[u8]) -> *mut i8 {
    let insize = inp.len();
    unsafe {
        let out = lodepng_malloc(insize + 1) as *mut i8;
        for i in 0..insize {
            *out.offset(i as isize) = inp[i] as i8;
        }
        *out.offset(insize as isize) = 0;
        out
    }
}

#[inline]
fn lodepng_read32bitInt(buffer: &[u8]) -> u32 {
    ((buffer[0] as u32) << 24) | ((buffer[1] as u32) << 16) | ((buffer[2] as u32) << 8) | buffer[3] as u32
}

fn lodepng_set32bitInt(buffer: &mut [u8], value: u32) {
    buffer[0] = ((value >> 24) & 255) as u8;
    buffer[1] = ((value >> 16) & 255) as u8;
    buffer[2] = ((value >> 8) & 255) as u8;
    buffer[3] = ((value) & 255) as u8;
}

fn add32bitInt(buffer: &mut Vec<u8>, value: u32) {
    buffer.push(((value >> 24) & 255) as u8);
    buffer.push(((value >> 16) & 255) as u8);
    buffer.push(((value >> 8) & 255) as u8);
    buffer.push(((value) & 255) as u8);
}

#[inline]
fn lodepng_add32bitInt(buffer: &mut ucvector, value: u32) {
    let n = buffer.len();
    buffer.resize(n + 4).unwrap();
    lodepng_set32bitInt(&mut buffer.slice_mut()[n..], value);
}

pub(crate) fn Text_copy(dest: &mut Info, source: &Info) -> Result<(), Error> {
    dest.text_keys = ptr::null_mut();
    dest.text_strings = ptr::null_mut();
    dest.text_num = 0;
    for (k, v) in source.text_keys_cstr() {
        dest.push_text(string_copy(k), string_copy(v))?;
    }
    Ok(())
}

unsafe fn realloc_push<T>(array: &mut *mut T, len: usize, new: T) -> Result<(), Error> {
    *array = lodepng_realloc((*array) as *mut _, (len + 1) * mem::size_of::<T>()) as *mut _;
    *array.offset(len as isize) = new;
    Ok(())
}


use ChunkPosition;

impl Info {

    pub(crate) fn push_itext(&mut self, key: *mut i8, langtag: *mut i8, transkey: *mut i8, str: *mut i8) -> Result<(), Error> {
        assert!(!key.is_null());
        assert!(!langtag.is_null());
        assert!(!transkey.is_null());
        assert!(!str.is_null());
        unsafe {
            realloc_push(&mut self.itext_keys, self.itext_num, key)?;
            realloc_push(&mut self.itext_langtags, self.itext_num, langtag)?;
            realloc_push(&mut self.itext_transkeys, self.itext_num, transkey)?;
            realloc_push(&mut self.itext_strings, self.itext_num, str)?;
            self.itext_num += 1;
        }
        Ok(())
    }

    pub(crate) fn push_text(&mut self, k: *mut i8, v: *mut i8) -> Result<(), Error> {
        assert!(!k.is_null());
        assert!(!v.is_null());
        unsafe {
            realloc_push(&mut self.text_keys, self.text_num, k)?;
            realloc_push(&mut self.text_strings, self.text_num, v)?;
            self.text_num += 1;
        }
        Ok(())
    }

    fn push_unknown_chunk(&mut self, critical_pos: ChunkPosition, chunk: &[u8]) -> Result<(), Error> {
        let set = critical_pos as usize;
        unsafe {
            let mut tmp = ucvector::from_raw(&mut self.unknown_chunks_data[set], self.unknown_chunks_size[set]);
            chunk_append(&mut tmp, chunk)?;
            let (data, size) = tmp.into_raw();
            self.unknown_chunks_data[set] = data;
            self.unknown_chunks_size[set] = size;
        }
        Ok(())
    }

    #[inline]
    fn unknown_chunks_data(&self, critical_pos: ChunkPosition) -> Option<&[u8]> {
        let set = critical_pos as usize;
        unsafe {
            if self.unknown_chunks_data[set].is_null() {
                return None;
            }
            Some(slice::from_raw_parts(self.unknown_chunks_data[set], self.unknown_chunks_size[set]))
        }
    }
}

pub fn lodepng_add_text(info: &mut Info, key: &CStr, str: &CStr) -> Result<(), Error> {
    info.push_text(string_copy(key), string_copy(str))
}

pub(crate) fn LodePNGIText_copy(dest: &mut Info, source: &Info) -> Result<(), Error> {
    dest.itext_keys = ptr::null_mut();
    dest.itext_langtags = ptr::null_mut();
    dest.itext_transkeys = ptr::null_mut();
    dest.itext_strings = ptr::null_mut();
    dest.itext_num = 0;
    for (k,l,t,s) in source.itext_keys() {
        dest.push_itext(string_copy_slice(k.as_bytes()),
            string_copy_slice(l.as_bytes()),
            string_copy_slice(t.as_bytes()),
            string_copy_slice(s.as_bytes()))?;
    }
    Ok(())
}

pub fn lodepng_add_itext(info: &mut Info, key: &CStr, langtag: &CStr, transkey: &CStr, str: &CStr) -> Result<(), Error> {
    info.push_itext(string_copy(key), string_copy(langtag), string_copy(transkey), string_copy(str))
}

fn addColorBits(out: &mut [u8], index: usize, bits: u32, mut inp: u32) {
    let m = match bits {
        1 => 7,
        2 => 3,
        _ => 1,
    };
    /*p = the partial index in the byte, e.g. with 4 palettebits it is 0 for first half or 1 for second half*/
    let p = index & m; /*filter out any other bits of the input value*/
    inp &= (1 << bits) - 1;
    inp <<= bits * (m - p) as u32;
    if p == 0 {
        out[index * bits as usize / 8] = inp as u8;
    } else {
        out[index * bits as usize / 8] |= inp as u8;
    }
}

pub type ColorTree = HashMap<(u8,u8,u8,u8), u16>;

fn rgba8ToPixel(out: &mut [u8], i: usize, mode: &ColorMode, tree: &mut ColorTree, /*for palette*/ r: u8, g: u8, b: u8, a: u8) -> Result<(), Error> {
    match mode.colortype {
        ColorType::GREY => {
            let grey = r; /*((unsigned short)r + g + b) / 3*/
            if mode.bitdepth() == 8 {
                out[i] = grey; /*take the most significant bits of grey*/
            } else if mode.bitdepth() == 16 {
                out[i * 2 + 0] = {
                    out[i * 2 + 1] = grey; /*color not in palette*/
                    out[i * 2 + 1]
                }; /*((unsigned short)r + g + b) / 3*/
            } else {
                let grey = (grey >> (8 - mode.bitdepth())) & ((1 << mode.bitdepth()) - 1); /*no error*/
                addColorBits(out, i, mode.bitdepth(), grey.into());
            };
        },
        ColorType::RGB => if mode.bitdepth() == 8 {
            out[i * 3 + 0] = r;
            out[i * 3 + 1] = g;
            out[i * 3 + 2] = b;
        } else {
            out[i * 6 + 0] = r;
            out[i * 6 + 1] = r;
            out[i * 6 + 2] = g;
            out[i * 6 + 3] = g;
            out[i * 6 + 4] = b;
            out[i * 6 + 5] = b;
        },
        ColorType::PALETTE => {
            let index = *tree.get(&(r, g, b, a)).ok_or(Error(82))?;
            if mode.bitdepth() == 8 {
                out[i] = index as u8;
            } else {
                addColorBits(out, i, mode.bitdepth(), index as u32);
            };
        },
        ColorType::GREY_ALPHA => {
            let grey = r;
            if mode.bitdepth() == 8 {
                out[i * 2 + 0] = grey;
                out[i * 2 + 1] = a;
            } else if mode.bitdepth() == 16 {
                out[i * 4 + 0] = grey;
                out[i * 4 + 1] = grey;
                out[i * 4 + 2] = a;
                out[i * 4 + 3] = a;
            }
        },
        ColorType::RGBA => if mode.bitdepth() == 8 {
            out[i * 4 + 0] = r;
            out[i * 4 + 1] = g;
            out[i * 4 + 2] = b;
            out[i * 4 + 3] = a;
        } else {
            out[i * 8 + 0] = r;
            out[i * 8 + 1] = r;
            out[i * 8 + 2] = g;
            out[i * 8 + 3] = g;
            out[i * 8 + 4] = b;
            out[i * 8 + 5] = b;
            out[i * 8 + 6] = a;
            out[i * 8 + 7] = a;
        },
        ColorType::BGRA |
        ColorType::BGR |
        ColorType::BGRX => {
            return Err(Error(31));
        },
    };
    Ok(())
}

/*put a pixel, given its RGBA16 color, into image of any color 16-bitdepth type*/
fn rgba16ToPixel(out: &mut [u8], i: usize, mode: &ColorMode, r: u16, g: u16, b: u16, a: u16) {
    match mode.colortype {
        ColorType::GREY => {
            let grey = r;
            out[i * 2 + 0] = (grey >> 8) as u8;
            out[i * 2 + 1] = grey as u8;
        },
        ColorType::RGB => {
            out[i * 6 + 0] = (r >> 8) as u8;
            out[i * 6 + 1] = r as u8;
            out[i * 6 + 2] = (g >> 8) as u8;
            out[i * 6 + 3] = g as u8;
            out[i * 6 + 4] = (b >> 8) as u8;
            out[i * 6 + 5] = b as u8;
        },
        ColorType::GREY_ALPHA => {
            let grey = r;
            out[i * 4 + 0] = (grey >> 8) as u8;
            out[i * 4 + 1] = grey as u8;
            out[i * 4 + 2] = (a >> 8) as u8;
            out[i * 4 + 3] = a as u8;
        },
        ColorType::RGBA => {
            out[i * 8 + 0] = (r >> 8) as u8;
            out[i * 8 + 1] = r as u8;
            out[i * 8 + 2] = (g >> 8) as u8;
            out[i * 8 + 3] = g as u8;
            out[i * 8 + 4] = (b >> 8) as u8;
            out[i * 8 + 5] = b as u8;
            out[i * 8 + 6] = (a >> 8) as u8;
            out[i * 8 + 7] = a as u8;
        },
        ColorType::BGR |
        ColorType::BGRA |
        ColorType::BGRX |
        ColorType::PALETTE => unreachable!(),
    };
}


/*Get RGBA8 color of pixel with index i (y * width + x) from the raw image with given color type.*/
fn getPixelColorRGBA8(inp: &[u8], i: usize, mode: &ColorMode) -> (u8,u8,u8,u8) {
    match mode.colortype {
        ColorType::GREY => {
            if mode.bitdepth() == 8 {
                let t = inp[i];
                let a = if mode.key() == Some((t as u16, t as u16, t as u16)) {
                    0
                } else {
                    255
                };
                (t, t, t, a)
            } else if mode.bitdepth() == 16 {
                let t = inp[i * 2 + 0];
                let g = 256 * inp[i * 2 + 0] as u16 + inp[i * 2 + 1] as u16;
                let a = if mode.key() == Some((g, g, g)) {
                    0
                } else {
                    255
                };
                (t, t, t, a)
            } else {
                let highest = (1 << mode.bitdepth()) - 1;
                /*highest possible value for this bit depth*/
                let mut j = i as usize * mode.bitdepth() as usize;
                let value = readBitsFromReversedStream(&mut j, inp, mode.bitdepth() as usize);
                let t = ((value * 255) / highest) as u8;
                let a = if mode.key() == Some((t as u16, t as u16, t as u16)) {
                    0
                } else {
                    255
                };
                (t, t, t, a)
            }
        },
        ColorType::RGB => if mode.bitdepth() == 8 {
            let r = inp[i * 3 + 0];
            let g = inp[i * 3 + 1];
            let b = inp[i * 3 + 2];
            let a = if mode.key() == Some((r as u16, g as u16, b as u16)) {
                0
            } else {
                255
            };
            (r, g, b, a)
        } else {
            (
                inp[i * 6 + 0],
                inp[i * 6 + 2],
                inp[i * 6 + 4],
                if mode.key()
                    == Some((
                        256 * inp[i * 6 + 0] as u16 + inp[i * 6 + 1] as u16,
                        256 * inp[i * 6 + 2] as u16 + inp[i * 6 + 3] as u16,
                        256 * inp[i * 6 + 4] as u16 + inp[i * 6 + 5] as u16,
                    )) {
                    0
                } else {
                    255
                },
            )
        },
        ColorType::PALETTE => {
            let index = if mode.bitdepth() == 8 {
                inp[i] as usize
            } else {
                let mut j = i as usize * mode.bitdepth() as usize;
                readBitsFromReversedStream(&mut j, inp, mode.bitdepth() as usize) as usize
            };
            let pal = mode.palette();
            if index >= pal.len() {
                /*This is an error according to the PNG spec, but common PNG decoders make it black instead.
                  Done here too, slightly faster due to no error handling needed.*/
                (0, 0, 0, 255)
            } else {
                let p = pal[index];
                (p.r, p.g, p.b, p.a)
            }
        },
        ColorType::GREY_ALPHA => if mode.bitdepth() == 8 {
            let t = inp[i * 2 + 0];
            (t, t, t, inp[i * 2 + 1])
        } else {
            let t = inp[i * 4 + 0];
            (t, t, t, inp[i * 4 + 2])
        },
        ColorType::RGBA => if mode.bitdepth() == 8 {
            (inp[i * 4 + 0], inp[i * 4 + 1], inp[i * 4 + 2], inp[i * 4 + 3])
        } else {
            (inp[i * 8 + 0], inp[i * 8 + 2], inp[i * 8 + 4], inp[i * 8 + 6])
        },
        ColorType::BGRA => {
            (inp[i * 4 + 2], inp[i * 4 + 1], inp[i * 4 + 0], inp[i * 4 + 3])
        },
        ColorType::BGR => {
            let b = inp[i * 3 + 0];
            let g = inp[i * 3 + 1];
            let r = inp[i * 3 + 2];
            let a = if mode.key() == Some((r as u16, g as u16, b as u16)) {
                0
            } else {
                255
            };
            (r, g, b, a)
        },
        ColorType::BGRX => {
            let b = inp[i * 4 + 0];
            let g = inp[i * 4 + 1];
            let r = inp[i * 4 + 2];
            let a = if mode.key() == Some((r as u16, g as u16, b as u16)) {
                0
            } else {
                255
            };
            (r, g, b, a)
        }
    }
}
/*Similar to getPixelColorRGBA8, but with all the for loops inside of the color
mode test cases, optimized to convert the colors much faster, when converting
to RGBA or RGB with 8 bit per cannel. buffer must be RGBA or RGB output with
enough memory, if has_alpha is true the output is RGBA. mode has the color mode
of the input buffer.*/
fn getPixelColorsRGBA8(buffer: &mut [u8], numpixels: usize, has_alpha: bool, inp: &[u8], mode: &ColorMode) {
    let num_channels = if has_alpha { 4 } else { 3 };
    match mode.colortype {
        ColorType::GREY => {
            if mode.bitdepth() == 8 {
                for (i, buffer) in buffer.chunks_mut(num_channels).take(numpixels).enumerate() {
                    buffer[0] = inp[i];
                    buffer[1] = inp[i];
                    buffer[2] = inp[i];
                    if has_alpha {
                        let a = inp[i] as u16;
                        buffer[3] = if mode.key() == Some((a, a, a)) {
                            0
                        } else {
                            255
                        };
                    }
                }
            } else if mode.bitdepth() == 16 {
                for (i, buffer) in buffer.chunks_mut(num_channels).take(numpixels).enumerate() {
                    buffer[0] = inp[i * 2];
                    buffer[1] = inp[i * 2];
                    buffer[2] = inp[i * 2];
                    if has_alpha {
                        let a = 256 * inp[i * 2 + 0] as u16 + inp[i * 2 + 1] as u16;
                        buffer[3] = if mode.key() == Some((a, a, a)) {
                            0
                        } else {
                            255
                        };
                    };
                }
            } else {
                let highest = (1 << mode.bitdepth()) - 1;
                /*highest possible value for this bit depth*/
                let mut j = 0;
                for buffer in buffer.chunks_mut(num_channels).take(numpixels) {
                    let value = readBitsFromReversedStream(&mut j, inp, mode.bitdepth() as usize);
                    buffer[0] = ((value * 255) / highest) as u8;
                    buffer[1] = ((value * 255) / highest) as u8;
                    buffer[2] = ((value * 255) / highest) as u8;
                    if has_alpha {
                        let a = value as u16;
                        buffer[3] = if mode.key() == Some((a, a, a)) {
                            0
                        } else {
                            255
                        };
                    };
                }
            };
        },
        ColorType::RGB => {
            if mode.bitdepth() == 8 {
                for (i, buffer) in buffer.chunks_mut(num_channels).take(numpixels).enumerate() {
                    buffer[0] = inp[i * 3 + 0];
                    buffer[1] = inp[i * 3 + 1];
                    buffer[2] = inp[i * 3 + 2];
                    if has_alpha {
                        buffer[3] = if mode.key() == Some((buffer[0] as u16, buffer[1] as u16, buffer[2] as u16)) {
                            0
                        } else {
                            255
                        };
                    };
                }
            } else {
                for (i, buffer) in buffer.chunks_mut(num_channels).take(numpixels).enumerate() {
                    buffer[0] = inp[i * 6 + 0];
                    buffer[1] = inp[i * 6 + 2];
                    buffer[2] = inp[i * 6 + 4];
                    if has_alpha {
                        let r = 256 * inp[i * 6 + 0] as u16 + inp[i * 6 + 1] as u16;
                        let g = 256 * inp[i * 6 + 2] as u16 + inp[i * 6 + 3] as u16;
                        let b = 256 * inp[i * 6 + 4] as u16 + inp[i * 6 + 5] as u16;
                        buffer[3] = if mode.key() == Some((r, g, b)) {
                            0
                        } else {
                            255
                        };
                    };
                }
            };
        },
        ColorType::PALETTE => {
            let mut j = 0;
            for (i, buffer) in buffer.chunks_mut(num_channels).take(numpixels).enumerate() {
                let index = if mode.bitdepth() == 8 {
                    inp[i] as usize
                } else {
                    readBitsFromReversedStream(&mut j, inp, mode.bitdepth() as usize) as usize
                };
                let pal = mode.palette();
                if index >= pal.len() {
                    /*This is an error according to the PNG spec, but most PNG decoders make it black instead.
                        Done here too, slightly faster due to no error handling needed.*/
                    buffer[0] = 0;
                    buffer[1] = 0;
                    buffer[2] = 0;
                    if has_alpha {
                        buffer[3] = 255u8;
                    }
                } else {
                    let p = pal[index as usize];
                    buffer[0] = p.r;
                    buffer[1] = p.g;
                    buffer[2] = p.b;
                    if has_alpha {
                        buffer[3] = p.a;
                    }
                };
            }
        },
        ColorType::GREY_ALPHA => if mode.bitdepth() == 8 {
            for (i, buffer) in buffer.chunks_mut(num_channels).take(numpixels).enumerate() {
                buffer[0] = inp[i * 2 + 0];
                buffer[1] = inp[i * 2 + 0];
                buffer[2] = inp[i * 2 + 0];
                if has_alpha {
                    buffer[3] = inp[i * 2 + 1];
                };
            }
        } else {
            for (i, buffer) in buffer.chunks_mut(num_channels).take(numpixels).enumerate() {
                buffer[0] = inp[i * 4 + 0];
                buffer[1] = inp[i * 4 + 0];
                buffer[2] = inp[i * 4 + 0];
                if has_alpha {
                    buffer[3] = inp[i * 4 + 2];
                };
            }
        },
        ColorType::RGBA => if mode.bitdepth() == 8 {
            for (i, buffer) in buffer.chunks_mut(num_channels).take(numpixels).enumerate() {
                buffer[0] = inp[i * 4 + 0];
                buffer[1] = inp[i * 4 + 1];
                buffer[2] = inp[i * 4 + 2];
                if has_alpha {
                    buffer[3] = inp[i * 4 + 3];
                }
            }
        } else {
            for (i, buffer) in buffer.chunks_mut(num_channels).take(numpixels).enumerate() {
                buffer[0] = inp[i * 8 + 0];
                buffer[1] = inp[i * 8 + 2];
                buffer[2] = inp[i * 8 + 4];
                if has_alpha {
                    buffer[3] = inp[i * 8 + 6];
                }
            }
        },
        ColorType::BGR => {
            for (i, buffer) in buffer.chunks_mut(num_channels).take(numpixels).enumerate() {
                buffer[0] = inp[i * 3 + 2];
                buffer[1] = inp[i * 3 + 1];
                buffer[2] = inp[i * 3 + 0];
                if has_alpha {
                    buffer[3] = if mode.key() == Some((buffer[0] as u16, buffer[1] as u16, buffer[2] as u16)) {
                        0
                    } else {
                        255
                    };
                };
            }
        },
        ColorType::BGRX => {
            for (i, buffer) in buffer.chunks_mut(num_channels).take(numpixels).enumerate() {
                buffer[0] = inp[i * 4 + 2];
                buffer[1] = inp[i * 4 + 1];
                buffer[2] = inp[i * 4 + 0];
                if has_alpha {
                    buffer[3] = if mode.key() == Some((buffer[0] as u16, buffer[1] as u16, buffer[2] as u16)) {
                        0
                    } else {
                        255
                    };
                };
            }
        },
        ColorType::BGRA => {
            for (i, buffer) in buffer.chunks_mut(num_channels).take(numpixels).enumerate() {
                buffer[0] = inp[i * 4 + 2];
                buffer[1] = inp[i * 4 + 1];
                buffer[2] = inp[i * 4 + 0];
                if has_alpha {
                    buffer[3] = inp[i * 4 + 3];
                }
            }
        },
    };
}
/*Get RGBA16 color of pixel with index i (y * width + x) from the raw image with
given color type, but the given color type must be 16-bit itself.*/
fn getPixelColorRGBA16(inp: &[u8], i: usize, mode: &ColorMode) -> (u16,u16,u16,u16) {
    match mode.colortype {
        ColorType::GREY => {
            let t = 256 * inp[i * 2 + 0] as u16 + inp[i * 2 + 1] as u16;
            (t,t,t,
            if mode.key() == Some((t,t,t)) {
                0
            } else {
                0xffff
            })
        },
        ColorType::RGB => {
            let r = 256 * inp[i * 6 + 0] as u16 + inp[i * 6 + 1] as u16;
            let g = 256 * inp[i * 6 + 2] as u16 + inp[i * 6 + 3] as u16;
            let b = 256 * inp[i * 6 + 4] as u16 + inp[i * 6 + 5] as u16;
            let a = if mode.key() == Some((r, g, b)) {
                0
            } else {
                0xffff
            };
            (r, g, b, a)
        },
        ColorType::GREY_ALPHA => {
            let t = 256 * inp[i * 4 + 0] as u16 + inp[i * 4 + 1] as u16;
            let a = 256 * inp[i * 4 + 2] as u16 + inp[i * 4 + 3] as u16;
            (t, t, t, a)
        },
        ColorType::RGBA => (
            256 * inp[i * 8 + 0] as u16 + inp[i * 8 + 1] as u16,
            256 * inp[i * 8 + 2] as u16 + inp[i * 8 + 3] as u16,
            256 * inp[i * 8 + 4] as u16 + inp[i * 8 + 5] as u16,
            256 * inp[i * 8 + 6] as u16 + inp[i * 8 + 7] as u16,
        ),
        ColorType::BGR |
        ColorType::BGRA |
        ColorType::BGRX |
        ColorType::PALETTE => unreachable!(),
    }
}

fn readBitsFromReversedStream(bitpointer: &mut usize, bitstream: &[u8], nbits: usize) -> u32 {
    let mut result = 0;
    for _ in 0..nbits {
        result <<= 1;
        result |= readBitFromReversedStream(bitpointer, bitstream) as u32;
    }
    result
}


fn readChunk_PLTE(color: &mut ColorMode, data: &[u8]) -> Result<(), Error> {
    color.palette_clear();
    for c in data.chunks(3).take(data.len() / 3) {
        color.palette_add(RGBA {
            r: c[0],
            g: c[1],
            b: c[2],
            a: 255,
        })?;
    }
    Ok(())
}

fn readChunk_tRNS(color: &mut ColorMode, data: &[u8]) -> Result<(), Error> {
    if color.colortype == ColorType::PALETTE {
        let pal = color.palette_mut();
        if data.len() > pal.len() {
            return Err(Error(38));
        }
        for (i, &d) in data.iter().enumerate() {
            pal[i].a = d;
        }
    } else if color.colortype == ColorType::GREY {
        if data.len() != 2 {
            return Err(Error(30));
        }
        let t = 256 * data[0] as u16 + data[1] as u16;
        color.set_key(t, t, t);
    } else if color.colortype == ColorType::RGB {
        if data.len() != 6 {
            return Err(Error(41));
        }
        color.set_key(
            256 * data[0] as u16 + data[1] as u16,
            256 * data[2] as u16 + data[3] as u16,
            256 * data[4] as u16 + data[5] as u16,
        );
    } else {
        return Err(Error(42));
    }
    Ok(())
}

/*background color chunk (bKGD)*/
fn readChunk_bKGD(info: &mut Info, data: &[u8]) -> Result<(), Error> {
    let chunkLength = data.len();
    if info.color.colortype == ColorType::PALETTE {
        /*error: this chunk must be 1 byte for indexed color image*/
        if chunkLength != 1 {
            return Err(Error(43)); /*error: this chunk must be 2 bytes for greyscale image*/
        } /*error: this chunk must be 6 bytes for greyscale image*/
        info.background_defined = 1; /* OK */
        info.background_r = {
            info.background_g = {
                info.background_b = data[0] as u32;
                info.background_b
            };
            info.background_g
        };
    } else if info.color.colortype == ColorType::GREY || info.color.colortype == ColorType::GREY_ALPHA {
        if chunkLength != 2 {
            return Err(Error(44));
        }
        info.background_defined = 1;
        info.background_r = {
            info.background_g = {
                info.background_b = 256 * data[0] as u32 + data[1] as u32;
                info.background_b
            };
            info.background_g
        };
    } else if info.color.colortype == ColorType::RGB || info.color.colortype == ColorType::RGBA {
        if chunkLength != 6 {
            return Err(Error(45));
        }
        info.background_defined = 1;
        info.background_r = 256 * data[0] as u32 + data[1] as u32;
        info.background_g = 256 * data[2] as u32 + data[3] as u32;
        info.background_b = 256 * data[4] as u32 + data[5] as u32;
    }
    Ok(())
}
/*text chunk (tEXt)*/
fn readChunk_tEXt(info: &mut Info, data: &[u8]) -> Result<(), Error> {
    let (keyword, str) = split_at_nul(data);
    if keyword.len() < 1 || keyword.len() > 79 {
        return Err(Error(89));
    }
    /*even though it's not allowed by the standard, no error is thrown if
        there's no null termination char, if the text is empty*/
    info.push_text(string_copy_slice(keyword), string_copy_slice(str))
}

/*compressed text chunk (zTXt)*/
fn readChunk_zTXt(info: &mut Info, zlibsettings: &DecompressSettings, data: &[u8]) -> Result<(), Error> {
    let mut length = 0;
    while length < data.len() && data[length] != 0 {
        length += 1
    }
    if length + 2 >= data.len() {
        return Err(Error(75));
    }
    if length < 1 || length > 79 {
        return Err(Error(89));
    }
    let key = &data[0..length];
    if data[length + 1] != 0 {
        return Err(Error(72));
    }
    /*the 0 byte indicating compression must be 0*/
    let string2_begin = length + 2; /*no null termination, corrupt?*/
    if string2_begin > data.len() {
        return Err(Error(75)); /*will fail if zlib error, e.g. if length is too small*/
    }
    let inl = &data[string2_begin..];
    let decoded = zlib_decompress(inl, zlibsettings)?;
    info.push_text(string_copy_slice(key), string_copy_slice(decoded.slice()))?;
    Ok(())
}

fn split_at_nul(data: &[u8]) -> (&[u8], &[u8]) {
    let mut part = data.splitn(2, |&b| b == 0);
    (part.next().unwrap(), part.next().unwrap_or(&data[0..0]))
}

/*international text chunk (iTXt)*/
fn readChunk_iTXt(info: &mut Info, zlibsettings: &DecompressSettings, data: &[u8]) -> Result<(), Error> {
    /*Quick check if the chunk length isn't too small. Even without check
        it'd still fail with other error checks below if it's too short. This just gives a different error code.*/
    if data.len() < 5 {
        /*iTXt chunk too short*/
        return Err(Error(30));
    }

    let (key, data) = split_at_nul(data);
    if key.is_empty() || key.len() > 79 {
        return Err(Error(89));
    }
    if data.len() < 2 {
        return Err(Error(75));
    }
    let compressed_flag = data[0];
    if data[1] != 0 {
        return Err(Error(72));
    }
    let (langtag, data) = split_at_nul(&data[2..]);
    let (transkey, data) = split_at_nul(data);

    let rest = if compressed_flag != 0 {
        let decoded = zlib_decompress(data, zlibsettings)?;
        string_copy_slice(decoded.slice())
    } else {
        string_copy_slice(data)
    };
    info.push_itext(string_copy_slice(key), string_copy_slice(langtag), string_copy_slice(transkey), rest)?;
    Ok(())
}

fn readChunk_tIME(info: &mut Info, data: &[u8]) -> Result<(), Error> {
    let chunkLength = data.len();
    if chunkLength != 7 {
        return Err(Error(73));
    }
    info.time_defined = 1;
    info.time.year = 256 * data[0] as u32 + data[1] as u32;
    info.time.month = data[2] as u32;
    info.time.day = data[3] as u32;
    info.time.hour = data[4] as u32;
    info.time.minute = data[5] as u32;
    info.time.second = data[6] as u32;
    Ok(())
}

fn readChunk_pHYs(info: &mut Info, data: &[u8]) -> Result<(), Error> {
    let chunkLength = data.len();
    if chunkLength != 9 {
        return Err(Error(74));
    }
    info.phys_defined = 1;
    info.phys_x = 16777216 * data[0] as u32 + 65536 * data[1] as u32 + 256 * data[2] as u32 + data[3] as u32;
    info.phys_y = 16777216 * data[4] as u32 + 65536 * data[5] as u32 + 256 * data[6] as u32 + data[7] as u32;
    info.phys_unit = data[8] as u32;
    Ok(())
}


fn addChunk_IDAT(out: &mut ucvector, data: &[u8], zlibsettings: &CompressSettings) -> Result<(), Error> {
    let zlib = zlib_compress(data, zlibsettings)?;
    addChunk(out, b"IDAT", zlib.slice())?;
    Ok(())
}

fn addChunk_IEND(out: &mut ucvector) -> Result<(), Error> {
    addChunk(out, b"IEND", &[])
}

fn addChunk_tEXt(out: &mut ucvector, keyword: &CStr, textstring: &CStr) -> Result<(), Error> {
    if keyword.to_bytes().len() < 1 || keyword.to_bytes().len() > 79 {
        return Err(Error(89));
    }
    let mut text = Vec::from(keyword.to_bytes_with_nul());
    text.extend_from_slice(textstring.to_bytes());
    addChunk(out, b"tEXt", &text)
}

fn addChunk_zTXt(out: &mut ucvector, keyword: &CStr, textstring: &CStr, zlibsettings: &CompressSettings) -> Result<(), Error> {
    if keyword.to_bytes().len() < 1 || keyword.to_bytes().len() > 79 {
        return Err(Error(89));
    }
    let mut data = Vec::from(keyword.to_bytes_with_nul());
    data.push(0u8);
    let textstring = textstring.to_bytes();
    let v = zlib_compress(textstring, zlibsettings)?;
    data.extend_from_slice(v.slice());
    addChunk(out, b"zTXt", &data)?;
    Ok(())
}

fn addChunk_iTXt(
    out: &mut ucvector, compressed: bool, keyword: &str, langtag: &str, transkey: &str, textstring: &str, zlibsettings: &CompressSettings,
) -> Result<(), Error> {
    let k_len = keyword.len();
    if k_len < 1 || k_len > 79 {
        return Err(Error(89));
    }
    let mut data = Vec::new();
    data.extend_from_slice(keyword.as_bytes()); data.push(0);
    data.push(compressed as u8);
    data.push(0);
    data.extend_from_slice(langtag.as_bytes()); data.push(0);
    data.extend_from_slice(transkey.as_bytes()); data.push(0);
    if compressed {
        let compressed_data = zlib_compress(textstring.as_bytes(), zlibsettings)?;
        data.extend_from_slice(compressed_data.slice());
    } else {
        data.extend_from_slice(textstring.as_bytes());
    }
    addChunk(out, b"iTXt", &data)
}


fn addChunk_bKGD(out: &mut ucvector, info: &Info) -> Result<(), Error> {
    let mut bKGD = Vec::new();
    if info.color.colortype == ColorType::GREY || info.color.colortype == ColorType::GREY_ALPHA {
        bKGD.push((info.background_r >> 8) as u8);
        bKGD.push((info.background_r & 255) as u8);
    } else if info.color.colortype == ColorType::RGB || info.color.colortype == ColorType::RGBA {
        bKGD.push((info.background_r >> 8) as u8);
        bKGD.push((info.background_r & 255) as u8);
        bKGD.push((info.background_g >> 8) as u8);
        bKGD.push((info.background_g & 255) as u8);
        bKGD.push((info.background_b >> 8) as u8);
        bKGD.push((info.background_b & 255) as u8);
    } else if info.color.colortype == ColorType::PALETTE {
        bKGD.push((info.background_r & 255) as u8);
    }
    addChunk(out, b"bKGD", &bKGD)
}

fn addChunk_IHDR(out: &mut ucvector, w: usize, h: usize, colortype: ColorType, bitdepth: usize, interlace_method: u8) -> Result<(), Error> {
    let mut header = Vec::new();
    add32bitInt(&mut header, w as u32);
    add32bitInt(&mut header, h as u32);
    header.push(bitdepth as u8);
    header.push(colortype as u8);
    header.push(0u8);
    header.push(0u8);
    header.push(interlace_method);
    addChunk(out, b"IHDR", &header)
}

fn addChunk_tRNS(out: &mut ucvector, info: &ColorMode) -> Result<(), Error> {
    let mut tRNS = Vec::new();
    if info.colortype == ColorType::PALETTE {
        let palette = info.palette();
        let mut amount = palette.len();
        /*the tail of palette values that all have 255 as alpha, does not have to be encoded*/
        let mut i = palette.len();
        while i != 0 {
            if palette[i - 1].a == 255 {
                amount -= 1;
            } else {
                break;
            };
            i -= 1;
        }
        for p in &palette[0..amount] {
            tRNS.push(p.a);
        }
    } else if info.colortype == ColorType::GREY {
        if let Some((r, _, _)) = info.key() {
            tRNS.push((r >> 8) as u8);
            tRNS.push((r & 255) as u8);
        };
    } else if info.colortype == ColorType::RGB {
        if let Some((r, g, b)) = info.key() {
            tRNS.push((r >> 8) as u8);
            tRNS.push((r & 255) as u8);
            tRNS.push((g >> 8) as u8);
            tRNS.push((g & 255) as u8);
            tRNS.push((b >> 8) as u8);
            tRNS.push((b & 255) as u8);
        };
    }
    addChunk(out, b"tRNS", &tRNS)
}

fn addChunk_PLTE(out: &mut ucvector, info: &ColorMode) -> Result<(), Error> {
    let mut PLTE = Vec::new();
    for p in info.palette() {
        PLTE.push(p.r);
        PLTE.push(p.g);
        PLTE.push(p.b);
    }
    addChunk(out, b"PLTE", &PLTE)
}

fn addChunk_tIME(out: &mut ucvector, time: &Time) -> Result<(), Error> {
    let data = [
        (time.year >> 8) as u8,
        (time.year & 255) as u8,
        time.month as u8,
        time.day as u8,
        time.hour as u8,
        time.minute as u8,
        time.second as u8,
    ];
    addChunk(out, b"tIME", &data)
}

fn addChunk_pHYs(out: &mut ucvector, info: &Info) -> Result<(), Error> {
    let mut data = Vec::new();
    add32bitInt(&mut data, info.phys_x);
    add32bitInt(&mut data, info.phys_y);
    data.push(info.phys_unit as u8);
    addChunk(out, b"pHYs", &data)
}


/*chunkName must be string of 4 characters*/
pub(crate) fn addChunk(out: &mut ucvector, type_: &[u8; 4], data: &[u8]) -> Result<(), Error> {
    let length = data.len() as usize;
    if length > (1 << 31) {
        return Err(Error(77));
    }
    let previous_length = out.len();
    out.reserve(length + 12);
    /*1: length*/
    lodepng_add32bitInt(out, length as u32);
    /*2: chunk name (4 letters)*/
    out.extend_from_slice(&type_[..])?;
    /*3: the data*/
    out.extend_from_slice(data)?;
    /*4: CRC (of the chunkname characters and the data)*/
    lodepng_add32bitInt(out, 0);
    lodepng_chunk_generate_crc(&mut out.slice_mut()[previous_length..]);
    Ok(())
}

/*shared values used by multiple Adam7 related functions*/
pub const ADAM7_IX: [u32; 7] = [0, 4, 0, 2, 0, 1, 0];
/*x start values*/
pub const ADAM7_IY: [u32; 7] = [0, 0, 4, 0, 2, 0, 1];
/*y start values*/
pub const ADAM7_DX: [u32; 7] = [8, 8, 4, 4, 2, 2, 1];
/*x delta values*/
pub const ADAM7_DY: [u32; 7] = [8, 8, 8, 4, 4, 2, 2];

fn Adam7_getpassvalues(w: usize, h: usize, bpp: usize) -> ([u32; 7], [u32; 7], [usize; 8], [usize; 8], [usize; 8]) {
    let mut passw: [u32; 7] = [0; 7];
    let mut passh: [u32; 7] = [0; 7];
    let mut filter_passstart: [usize; 8] = [0; 8];
    let mut padded_passstart: [usize; 8] = [0; 8];
    let mut passstart: [usize; 8] = [0; 8];

    /*the passstart values have 8 values: the 8th one indicates the byte after the end of the 7th (= last) pass*/
    /*calculate width and height in pixels of each pass*/
    for i in 0..7 {
        passw[i] = (w as u32 + ADAM7_DX[i] - ADAM7_IX[i] - 1) / ADAM7_DX[i]; /*if passw[i] is 0, it's 0 bytes, not 1 (no filtertype-byte)*/
        passh[i] = (h as u32 + ADAM7_DY[i] - ADAM7_IY[i] - 1) / ADAM7_DY[i]; /*bits padded if needed to fill full byte at end of each scanline*/
        if passw[i] == 0 {
            passh[i] = 0; /*only padded at end of reduced image*/
        }
        if passh[i] == 0 {
            passw[i] = 0;
        };
    }
    filter_passstart[0] = 0;
    padded_passstart[0] = 0;
    passstart[0] = 0;
    for i in 0..7 {
        filter_passstart[i + 1] = filter_passstart[i] + if passw[i] != 0 && passh[i] != 0 {
            passh[i] as usize * (1 + (passw[i] as usize * bpp + 7) / 8)
        } else {
            0
        };
        padded_passstart[i + 1] = padded_passstart[i] + passh[i] as usize * ((passw[i] as usize * bpp + 7) / 8) as usize;
        passstart[i + 1] = passstart[i] + (passh[i] as usize * passw[i] as usize * bpp + 7) / 8;
    }
    (passw, passh, filter_passstart, padded_passstart, passstart)
}

/*
in: Adam7 interlaced image, with no padding bits between scanlines, but between
 reduced images so that each reduced image starts at a byte.
out: the same pixels, but re-ordered so that they're now a non-interlaced image with size w*h
bpp: bits per pixel
out has the following size in bits: w * h * bpp.
in is possibly bigger due to padding bits between reduced images.
out must be big enough AND must be 0 everywhere if bpp < 8 in the current implementation
(because that's likely a little bit faster)
NOTE: comments about padding bits are only relevant if bpp < 8
*/
fn Adam7_deinterlace(out: &mut [u8], inp: &[u8], w: usize, h: usize, bpp: usize) {
    let (passw, passh, _, _, passstart) = Adam7_getpassvalues(w, h, bpp);
    if bpp >= 8 {
        for i in 0..7 {
            let bytewidth = bpp / 8;
            for y in 0..passh[i] {
                for x in 0..passw[i] {
                    let pixelinstart = passstart[i] + (y * passw[i] + x) as usize * bytewidth;
                    let pixeloutstart = ((ADAM7_IY[i] + y * ADAM7_DY[i]) as usize * w + ADAM7_IX[i] as usize + x as usize * ADAM7_DX[i] as usize) * bytewidth;

                    out[pixeloutstart..(bytewidth + pixeloutstart)]
                        .clone_from_slice(&inp[pixelinstart..(bytewidth + pixelinstart)])
                }
            }
        }
    } else {
        for i in 0..7 {
            let ilinebits = bpp * passw[i] as usize;
            let olinebits = bpp * w;
            for y in 0..passh[i] as usize {
                for x in 0..passw[i] as usize {
                    let mut ibp = (8 * passstart[i]) + (y * ilinebits + x * bpp) as usize;
                    let mut obp = ((ADAM7_IY[i] as usize + y * ADAM7_DY[i] as usize) * olinebits + (ADAM7_IX[i] as usize + x * ADAM7_DX[i] as usize) * bpp) as usize;
                    for _ in 0..bpp {
                        let bit = readBitFromReversedStream(&mut ibp, inp);
                        /*note that this function assumes the out buffer is completely 0, use setBitOfReversedStream otherwise*/
                        setBitOfReversedStream0(&mut obp, out, bit);
                    }
                }
            }
        }
    };
}

/* ////////////////////////////////////////////////////////////////////////// */
/* / Reading and writing single bits and bytes from/to stream for LodePNG   / */
/* ////////////////////////////////////////////////////////////////////////// */
fn readBitFromReversedStream(bitpointer: &mut usize, bitstream: &[u8]) -> u8 {
    let result = ((bitstream[(*bitpointer) >> 3] >> (7 - ((*bitpointer) & 7))) & 1) as u8;
    *bitpointer += 1;
    result
}

fn setBitOfReversedStream0(bitpointer: &mut usize, bitstream: &mut [u8], bit: u8) {
    /*the current bit in bitstream must be 0 for this to work*/
    if bit != 0 {
        /*earlier bit of huffman code is in a lesser significant bit of an earlier byte*/
        bitstream[(*bitpointer) >> 3] |= bit << (7 - ((*bitpointer) & 7));
    }
    *bitpointer += 1;
}


fn setBitOfReversedStream(bitpointer: &mut usize, bitstream: &mut [u8], bit: u8) {
    /*the current bit in bitstream may be 0 or 1 for this to work*/
    if bit == 0 {
        bitstream[(*bitpointer) >> 3] &= (!(1 << (7 - ((*bitpointer) & 7)))) as u8;
    } else {
        bitstream[(*bitpointer) >> 3] |= 1 << (7 - ((*bitpointer) & 7));
    }
    *bitpointer += 1;
}
/* ////////////////////////////////////////////////////////////////////////// */
/* / PNG chunks                                                             / */
/* ////////////////////////////////////////////////////////////////////////// */
pub fn lodepng_chunk_length(chunk: &[u8]) -> usize {
    lodepng_read32bitInt(chunk) as usize
}

pub fn lodepng_chunk_type(chunk: &[u8]) -> &[u8] {
    &chunk[4..8]
}

pub(crate) fn lodepng_chunk_data(chunk: &[u8]) -> Result<&[u8], Error> {
    let len = lodepng_chunk_length(chunk) as usize;
    /*error: chunk length larger than the max PNG chunk size*/
    if len > (1 << 31) {
        return Err(Error(63));
    }
    if chunk.len() < len + 12 {
        return Err(Error(64));
    }

    Ok(&chunk[8..8 + len])
}

pub(crate) fn lodepng_chunk_data_mut(chunk: &mut [u8]) -> Result<&mut [u8], Error> {
    let len = lodepng_chunk_length(chunk) as usize;
    /*error: chunk length larger than the max PNG chunk size*/
    if len > (1 << 31) {
        return Err(Error(63));
    }
    if chunk.len() < len + 12 {
        return Err(Error(64));
    }

    Ok(&mut chunk[8..8 + len])
}

pub(crate) fn lodepng_chunk_next(chunk: &[u8]) -> &[u8] {
    let total_chunk_length = lodepng_chunk_length(chunk) as usize + 12;
    &chunk[total_chunk_length..]
}

pub(crate) fn lodepng_chunk_next_mut(chunk: &mut [u8]) -> &mut [u8] {
    let total_chunk_length = lodepng_chunk_length(chunk) as usize + 12;
    &mut chunk[total_chunk_length..]
}

pub fn lodepng_chunk_ancillary(chunk: &[u8]) -> bool {
    (chunk[4] & 32) != 0
}

pub fn lodepng_chunk_private(chunk: &[u8]) -> bool {
    (chunk[6] & 32) != 0
}

pub fn lodepng_chunk_safetocopy(chunk: &[u8]) -> bool {
    (chunk[7] & 32) != 0
}

pub fn lodepng_chunk_check_crc(chunk: &[u8]) -> bool {
    let length = lodepng_chunk_length(chunk) as usize;
    /*the CRC is taken of the data and the 4 chunk type letters, not the length*/
    let CRC = lodepng_read32bitInt(&chunk[length + 8..]);
    let checksum = lodepng_crc32(&chunk[4..length + 8]);
    CRC == checksum
}

pub fn lodepng_chunk_generate_crc(chunk: &mut [u8]) {
    let length = lodepng_chunk_length(chunk) as usize;
    let CRC = lodepng_crc32(&chunk[4..length + 8]);
    lodepng_set32bitInt(&mut chunk[8 + length..], CRC);
}

pub(crate) fn chunk_append(out: &mut ucvector, chunk: &[u8]) -> Result<(), Error> {
    let total_chunk_length = lodepng_chunk_length(chunk) as usize + 12;
    out.extend_from_slice(&chunk[0..total_chunk_length])
}

/* ////////////////////////////////////////////////////////////////////////// */
/* / Color types and such                                                   / */
/* ////////////////////////////////////////////////////////////////////////// */
fn checkPngColorValidity(colortype: ColorType, bd: u32) -> Result<(), Error> {
    /*allowed color type / bits combination*/
    match colortype {
        ColorType::GREY => if !(bd == 1 || bd == 2 || bd == 4 || bd == 8 || bd == 16) {
            return Err(Error(37));
        },
        ColorType::PALETTE => if !(bd == 1 || bd == 2 || bd == 4 || bd == 8) {
            return Err(Error(37));
        },
        ColorType::RGB | ColorType::GREY_ALPHA | ColorType::RGBA => if !(bd == 8 || bd == 16) {
            return Err(Error(37));
        },
        _ => {
            return Err(Error(31))
        },
    }
    Ok(())
}
/// Internally BGRA is allowed
fn checkLodeColorValidity(colortype: ColorType, bd: u32) -> Result<(), Error> {
    match colortype {
        ColorType::BGRA | ColorType::BGRX | ColorType::BGR if bd == 8 => {
            Ok(())
        },
        ct => checkPngColorValidity(ct, bd),
    }
}

pub fn lodepng_color_mode_equal(a: &ColorMode, b: &ColorMode) -> bool {
    a.colortype == b.colortype &&
    a.bitdepth() == b.bitdepth() &&
    a.key() == b.key() &&
    a.palette() == b.palette()
}

/* ////////////////////////////////////////////////////////////////////////// */
/* / Deflator (Compressor)                                                  / */
/* ////////////////////////////////////////////////////////////////////////// */

const MAX_SUPPORTED_DEFLATE_LENGTH: usize = 258;
/*bitlen is the size in bits of the code*/
fn addHuffmanSymbol(bp: &mut usize, compressed: &mut ucvector, code: u32, bitlen: u32) {
    addBitsToStreamReversed(bp, compressed, code, bitlen as usize);
}

fn addBitsToStreamReversed(bitpointer: &mut usize, bitstream: &mut ucvector, value: u32, nbits: usize) {
    for i in 0..nbits {
        if ((*bitpointer) & 7) == 0 {
            bitstream.push(0u8);
        }
        let end = bitstream.len() - 1;
        bitstream.slice_mut()[end] |= (((value >> (nbits - 1 - i)) & 1) as u8) << ((*bitpointer) & 7);
        *bitpointer += 1;
    }
}

fn addBitsToStream(bitpointer: &mut usize, bitstream: &mut ucvector, value: u32, nbits: usize) {
    for i in 0..nbits {
        if ((*bitpointer) & 7) == 0 {
            bitstream.push(0u8);
        }
        let end = bitstream.len() - 1;
        bitstream.slice_mut()[end] |= (((value >> i) & 1) as u8) << ((*bitpointer) & 7);
        *bitpointer += 1;
    }
}

fn readBitFromStream(bitpointer: &mut usize, bitstream: &[u8]) -> u8 {
    let result = ((bitstream[*bitpointer >> 3] >> (*bitpointer & 7)) & 1u8) as u8;
    *bitpointer += 1;
    result
}

fn readBitsFromStream(bitpointer: &mut usize, bitstream: &[u8], nbits: usize) -> u32 {
    let mut result = 0;
    for i in 0..nbits {
        result += (((bitstream[*bitpointer >> 3] >> (*bitpointer & 7)) & 1u8) as u32) << i;
        *bitpointer += 1;
    }
    result
}

const NUM_DISTANCE_SYMBOLS: usize = 32;
/*get the distance code tree of a deflated block with fixed tree, as specified in the deflate specification*/
fn generateFixedDistanceTree() -> Result<HuffmanTree, Error> {
    let bitlen = vec![5; NUM_DISTANCE_SYMBOLS];
    HuffmanTree::from_lengths(&bitlen, 15)
}

/*get the tree of a deflated block with fixed tree, as specified in the deflate specification*/
fn getTreeInflateFixed() -> Result<(HuffmanTree, HuffmanTree), Error> {
    Ok((generateFixedLitLenTree()?, generateFixedDistanceTree()?))
}

pub const NUM_CODE_LENGTH_CODES: usize = 19;
/*the order in which "code length alphabet code lengths" are stored, out of this
the huffman tree of the dynamic huffman tree lengths is generated*/
pub const CLCL_ORDER: [u32; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];
pub const NUM_DEFLATE_CODE_SYMBOLS: usize = 288;
pub const FIRST_LENGTH_CODE_INDEX: u32 = 257;
pub const LAST_LENGTH_CODE_INDEX: u32 = 285;

fn inflateHuffmanBlock(out: &mut ucvector, inp: &[u8], bp: &mut usize, pos: &mut usize, btype: u32) -> Result<(), Error> {
    let (ref mut tree_ll, ref mut tree_d) = if btype == 1 {
        getTreeInflateFixed()?
    } else {
        assert_eq!(2, btype);
        getTreeInflateDynamic(inp, bp)?
    };
    loop {
        match tree_ll.decode_symbol(inp, bp) {
            Some(code_ll @ 0...255) => {
                out.push(code_ll as u8);
                (*pos) += 1;
            },
            Some(code_ll @ FIRST_LENGTH_CODE_INDEX...LAST_LENGTH_CODE_INDEX) => {
                let numextrabits_d; /*part 2: get extra bits and add the value of that to length*/
                let mut length: usize = LENGTHBASE[(code_ll - FIRST_LENGTH_CODE_INDEX) as usize] as usize;
                let numextrabits_l = LENGTHEXTRA[(code_ll - FIRST_LENGTH_CODE_INDEX) as usize] as usize;
                let inbitlength = inp.len() * 8;
                if (*bp + numextrabits_l) > inbitlength {
                    return Err(Error(51));
                }
                /*error, bit pointer will jump past memory*/
                length += readBitsFromStream(bp, inp, numextrabits_l) as usize; /*part 3: get distance code*/
                /*return error code 10 or 11 depending on the situation that happened in decode_symbol
                              (10=no endcode, 11=wrong jump outside of tree)*/
                let code_d = tree_d.decode_symbol(inp, bp).ok_or_else(|| Error(if (*bp) > inp.len() * 8 { 10 } else { 11 }))?;
                if code_d > 29 {
                    /*error: invalid distance code (30-31 are never used)*/
                    return Err(Error(18)); /*part 4: get extra bits from distance*/
                }
                let mut distance = DISTANCEBASE[code_d as usize] as usize;
                numextrabits_d = DISTANCEEXTRA[code_d as usize] as usize;
                let inbitlength = inp.len() * 8;
                if (*bp + numextrabits_d) > inbitlength {
                    return Err(Error(51));
                }
                /*error, bit pointer will jump past memory*/
                distance += readBitsFromStream(bp, inp, numextrabits_d) as usize; /*part 5: fill in all the out[n] values based on the length and dist*/
                let start = *pos; /*too long backward distance*/
                if distance > start {
                    return Err(Error(52));
                }
                let mut backward = start - distance;
                out.resize((*pos) + length)?;
                let out_data = out.slice_mut();
                if distance < length {
                    for _ in 0..length {
                        out_data[*pos] = out_data[backward];
                        backward += 1;
                        (*pos) += 1;
                    }
                } else {
                    let (src, dest) = out_data.split_at_mut(*pos);
                    let dest = &mut dest[0..length];
                    let src = &src[backward..backward + length];
                    dest.clone_from_slice(src);
                    *pos += length;
                }
            },
            Some(256) => {
                break;
            },
            _ => {
                /*return error code 10 or 11 depending on the situation that happened in decode_symbol
                      (10=no endcode, 11=wrong jump outside of tree)*/
                return Err(Error(if (*bp) > inp.len() * 8 { 10 } else { 11 }));
            },
        }
    }
    Ok(())
}

fn inflateNoCompression(out: &mut ucvector, inp: &[u8], bp: &mut usize, pos: &mut usize) -> Result<(), Error> {
    /*go to first boundary of byte*/
    while ((*bp) & 7) != 0 {
        (*bp) += 1; /*byte position*/
    }
    let mut p = (*bp) / 8;
    /*read LEN (2 bytes) and nlen (2 bytes)*/
    if p + 4 >= inp.len() {
        return Err(Error(52)); /*error, bit pointer will jump past memory*/
    }
    let len = inp[p] as usize + 256 * inp[p + 1] as usize;
    p += 2;
    let nlen = inp[p] as usize + 256 * inp[p + 1] as usize;
    p += 2;
    /*check if 16-bit nlen is really the one's complement of len*/
    if len + nlen != 65535 {
        return Err(Error(21)); /*error: nlen is not one's complement of len*/
    }
    /*read the literal data: len bytes are now stored in the out buffer*/
    if p + len > inp.len() {
        return Err(Error(23)); /*error: reading outside of in buffer*/
    }
    out.extend_from_slice(&inp[p..p + len])?;
    p += len;
    (*pos) += len;
    (*bp) = p * 8;
    Ok(())
}

fn getTreeInflateDynamic(inp: &[u8], bp: &mut usize) -> Result<(HuffmanTree, HuffmanTree), Error> {
    if (*bp) + 14 > (inp.len() << 3) {
        return Err(Error(49));
    }
    let HLIT = (readBitsFromStream(bp, inp, 5) + 257) as usize;
    let HDIST = (readBitsFromStream(bp, inp, 5) + 1) as usize;
    let HCLEN = (readBitsFromStream(bp, inp, 4) + 4) as usize;
    if (*bp) + HCLEN as usize * 3 > (inp.len() << 3) {
        return Err(Error(50));
    }
    let mut bitlen_cl = vec![0; NUM_CODE_LENGTH_CODES];
    for &clcl in CLCL_ORDER.iter().take(HCLEN) {
        bitlen_cl[clcl as usize] = readBitsFromStream(bp, inp, 3);
    }
    let tree_cl = HuffmanTree::from_lengths(&bitlen_cl, 7)?;
    /*now we can use this tree to read the lengths for the tree that this function will return*/
    let mut bitlen_ll = vec![0; NUM_DEFLATE_CODE_SYMBOLS];
    let mut bitlen_d = vec![0; NUM_DISTANCE_SYMBOLS];

    /*i is the current symbol we're reading in the part that contains the code lengths of lit/len and dist codes*/
    let mut i = 0;
    while i < HLIT + HDIST {
        /*repeat previous*/
        match tree_cl.decode_symbol(inp, bp) {
            Some(code @ 0...15) => {
                if i < HLIT {
                    bitlen_ll[i] = code; /*read in the 2 bits that indicate repeat length (3-6)*/
                } else {
                    bitlen_d[i - HLIT] = code; /*set value to the previous code*/
                } /*can't repeat previous if i is 0*/
                i += 1; /*error, bit pointer jumps past memory*/
            },
            Some(16) => {
                let mut replength = 3; /*repeat this value in the next lengths*/
                let value; /*error: i is larger than the amount of codes*/
                if i == 0 {
                    /*read in the bits that indicate repeat length*/
                    return Err(Error(54)); /*repeat "0" 3-10 times*/
                } /*error, bit pointer jumps past memory*/
                let inbitlength = inp.len() * 8;
                if (*bp + 2) > inbitlength {
                    /*error: i is larger than the amount of codes*/
                    return Err(Error(50)); /*repeat this value in the next lengths*/
                } /*repeat "0" 11-138 times*/
                replength += readBitsFromStream(bp, inp, 2); /*read in the bits that indicate repeat length*/
                if i < HLIT + 1 {
                    value = bitlen_ll[i - 1]; /*error, bit pointer jumps past memory*/
                } else {
                    value = bitlen_d[i - HLIT - 1]; /*repeat this value in the next lengths*/
                } /*error: i is larger than the amount of codes*/
                let mut n = 0; /*if(code == (unsigned)(-1))*/
                while n < replength {
                    if i >= HLIT + HDIST {
                        return Err(Error(13));
                    }
                    if i < HLIT {
                        bitlen_ll[i] = value;
                    } else {
                        bitlen_d[i - HLIT] = value;
                    }
                    i += 1;
                    n += 1
                }
            },
            Some(17) => {
                let mut replength = 3;
                let inbitlength = inp.len() * 8;
                if (*bp + 3) > inbitlength {
                    return Err(Error(50));
                }
                replength += readBitsFromStream(bp, inp, 3);
                let mut n = 0;
                while n < replength {
                    if i >= HLIT + HDIST {
                        return Err(Error(14));
                    }
                    if i < HLIT {
                        bitlen_ll[i] = 0;
                    } else {
                        bitlen_d[i - HLIT] = 0;
                    }
                    i += 1;
                    n += 1
                }
            },
            Some(18) => {
                let mut replength = 11;
                let inbitlength = inp.len() * 8;
                if (*bp + 7) > inbitlength {
                    return Err(Error(50));
                }
                replength += readBitsFromStream(bp, inp, 7);
                let mut n = 0;
                while n < replength {
                    if i >= HLIT + HDIST {
                        return Err(Error(15));
                    }
                    if i < HLIT {
                        bitlen_ll[i] = 0;
                    } else {
                        bitlen_d[i - HLIT] = 0;
                    }
                    i += 1;
                    n += 1;
                }
            },
            Some(_) => {
                return Err(Error({
                    /*return error code 10 or 11 depending on the situation that happened in huffmanDecodeSymbol
                          (10=no endcode, 11=wrong jump outside of tree)*/
                    let inbitlength = inp.len() * 8;
                    if (*bp) > inbitlength {
                        10
                    } else {
                        11
                    } /*unexisting code, this can never happen*/
                }));
            },
            _ => {
                return Err(Error(16));
            },
        };
    }
    if bitlen_ll[256] == 0 {
        return Err(Error(64));
    }
    /*the length of the end code 256 must be larger than 0*/
    /*now we've finally got HLIT and HDIST, so generate the code trees, and the function is done*/
    let tree_ll = HuffmanTree::from_lengths(&bitlen_ll, 15)?;
    let tree_d = HuffmanTree::from_lengths(&bitlen_d, 15)?;
    Ok((tree_ll, tree_d))
}

fn generateFixedLitLenTree() -> Result<HuffmanTree, Error> {
    let mut bitlen = vec![8; NUM_DEFLATE_CODE_SYMBOLS];
    /*288 possible codes: 0-255=literals, 256=endcode, 257-285=lengthcodes, 286-287=unused*/
    for b in &mut bitlen[144..256] {
        *b = 9;
    }
    for b in &mut bitlen[256..280] {
        *b = 7;
    }
    HuffmanTree::from_lengths(&bitlen, 15)
}

/* ////////////////////////////////////////////////////////////////////////// */
/* / Inflator (Decompressor)                                                / */
/* ////////////////////////////////////////////////////////////////////////// */


pub(crate) fn lodepng_inflatev(inp: &[u8], _settings: &DecompressSettings) -> Result<ucvector, Error> {
    let mut out = ucvector::new();
    /*bit pointer in the "in" data, current byte is bp >> 3, current bit is bp & 0x7 (from lsb to msb of the byte)*/
    let mut bp = 0; /*byte position in the out buffer*/
    let mut BFINAL = 0; /*error, bit pointer will jump past memory*/
    let mut pos = 0; /*error: invalid BTYPE*/
    while BFINAL == 0 {
        let mut BTYPE: u32;
        if bp + 2 >= inp.len() * 8 {
            return Err(Error(52));
        }
        BFINAL = readBitFromStream(&mut bp, inp);
        BTYPE = 1 * readBitFromStream(&mut bp, inp) as u32;
        BTYPE += 2 * readBitFromStream(&mut bp, inp) as u32;
        if BTYPE == 3 {
            return Err(Error(20));
        } else if BTYPE == 0 {
            /*no compression*/
            inflateNoCompression(&mut out, inp, &mut bp, &mut pos)?;
        } else {
            /*compression, BTYPE 01 or 10*/
            inflateHuffmanBlock(&mut out, inp, &mut bp, &mut pos, BTYPE)?;
        }
    }
    Ok(out)
}

fn inflate(inp: &[u8], settings: &DecompressSettings) -> Result<ucvector, Error> {
    if let Some(cb) = settings.custom_inflate {
        unsafe {
            let mut outdata = ptr::null_mut();
            let mut outsize = 0;
            Error((cb)(&mut outdata, &mut outsize, inp.as_ptr(), inp.len(), settings)).to_result()?;
            Ok(ucvector::from_raw(&mut outdata, outsize))
        }
    } else {
        lodepng_inflatev(inp, settings)
    }
}


/* ////////////////////////////////////////////////////////////////////////// */
/* / Deflator (Compressor)                                                  / */
/* ////////////////////////////////////////////////////////////////////////// */

/*search the index in the array, that has the largest value smaller than or equal to the given value,
given array must be sorted (if no value is smaller, it returns the size of the given array)*/
fn searchCodeIndex(array: &[u32], value: u32) -> u32 {
    let idx = match array.binary_search(&value) {Ok(x) | Err(x) => x};
    if idx > 0 && array[idx] > value {
        idx as u32 - 1
    } else {
        idx as u32
    }
}

/*values in encoded vector are those used by deflate:
  0-255: literal bytes
  256: end
  257-285: length/distance pair (length code, followed by extra length bits, distance code, extra distance bits)
  286-287: invalid*/
#[inline]
fn addLengthDistance(values: &mut Vec<u32>, length: u32, distance: u32) {
    let length_code = searchCodeIndex(&LENGTHBASE, length);
    let extra_length = length - LENGTHBASE[length_code as usize];
    let dist_code = searchCodeIndex(&DISTANCEBASE, distance);
    let extra_distance = distance - DISTANCEBASE[dist_code as usize];
    values.push(length_code + FIRST_LENGTH_CODE_INDEX);
    values.push(extra_length);
    values.push(dist_code);
    values.push(extra_distance);
}

pub const HASH_NUM_VALUES: usize = 65536;

struct Hash {
    pub head: Vec<i32>,
    pub chain: Vec<u16>,
    pub val: Vec<i32>,
    pub headz: Vec<i32>,
    pub chainz: Vec<u16>,
    pub zeros: Vec<u16>,
}

impl Hash {
    pub fn new(windowsize: usize) -> Result<Self, Error> {
        let mut hash = Hash {
            head: vec![-1; HASH_NUM_VALUES],
            val: vec![-1; windowsize],
            chain: vec![0; windowsize],
            chainz: vec![0; windowsize],
            zeros: vec![0; windowsize],
            headz: vec![-1; MAX_SUPPORTED_DEFLATE_LENGTH + 1],
        };
        /*same value as index indicates uninitialized*/
        for (i, c) in hash.chain.iter_mut().enumerate() {
            *c = i as u16;
        }
        for (i, c) in hash.chainz.iter_mut().enumerate() {
            *c = i as u16;
        }
        Ok(hash)
    }
}

#[inline(always)]
fn getHash(data: &[u8], pos: usize) -> u16 {
    let mut result = 0;
    if pos + 2 < data.len() {
        /*A simple shift and xor hash is used. Since the data of PNGs is dominated
            by zeroes due to the filters, a better hash does not have a significant
            effect on speed in traversing the chain, and causes more time spend on
            calculating the hash.*/
        result ^= (data[pos + 0] as u32) << 0;
        result ^= (data[pos + 1] as u32) << 4;
        result ^= (data[pos + 2] as u32) << 8;
    } else {
        if pos >= data.len() {
            return 0;
        }
        for i in 0..data.len() - pos {
            result ^= (Wrapping(data[pos + i]) << (i * 8)).0 as u32
        }
    }
    result as u16
}

#[inline(always)]
fn countZeros(data: &[u8], pos: usize) -> u32 {
    data[pos..].iter()
        .take(MAX_SUPPORTED_DEFLATE_LENGTH)
        .take_while(|&d| *d == 0)
        .count() as u32
}

#[inline]
fn updateHashChain(hash: &mut Hash, wpos: u32, hashval: u32, numzeros: u16) {
    hash.val[wpos as usize] = hashval as i32;
    if hash.head[hashval as usize] != -1 {
        hash.chain[wpos as usize] = hash.head[hashval as usize] as u16;
    }
    hash.head[hashval as usize] = wpos as i32;
    hash.zeros[wpos as usize] = numzeros;
    if hash.headz[numzeros as usize] != -1 {
        hash.chainz[wpos as usize] = hash.headz[numzeros as usize] as u16;
    }
    hash.headz[numzeros as usize] = wpos as i32;
}

fn deflateNoCompression(data: &[u8]) -> Result<ucvector, Error> {
    /*non compressed deflate block data: 1 bit BFINAL,2 bits BTYPE,(5 bits): it jumps to start of next byte,
      2 bytes LEN, 2 bytes nlen, LEN bytes literal DATA*/
    let numdeflateblocks = (data.len() + 65534) / 65535;
    let mut datapos = 0;
    let mut out = ucvector::new();
    for i in 0..numdeflateblocks {
        let bfinal = (i == numdeflateblocks - 1) as usize;
        let BTYPE = 0;
        let firstbyte = (bfinal + ((BTYPE & 1) << 1) + ((BTYPE & 2) << 1)) as u8;
        out.push(firstbyte);
        let len = (data.len() - datapos).min(65535);
        let nlen = 65535 - len;
        out.push((len & 255) as u8);
        out.push((len >> 8) as u8);
        out.push((nlen & 255) as u8);
        out.push((nlen >> 8) as u8);
        let mut j = 0;
        while j < 65535 && datapos < data.len() {
            out.push(data[datapos]);
            datapos += 1;
            j += 1
        }
    }
    Ok(out)
}

/*
write the lz77-encoded data, which has lit, len and dist codes, to compressed stream using huffman trees.
tree_ll: the tree for lit and len codes.
tree_d: the tree for distance codes.
*/
fn writeLZ77data(bp: &mut usize, out: &mut ucvector, lz77_encoded: &[u32], tree_ll: &HuffmanTree, tree_d: &HuffmanTree) {
    let mut i = 0; /*for a length code, 3 more things have to be added*/
    while i != lz77_encoded.len() {
        let val = lz77_encoded[i as usize];
        addHuffmanSymbol(bp, out, tree_ll.code(val), tree_ll.length(val));
        if val > 256 {
            let length_index = (val - FIRST_LENGTH_CODE_INDEX as u32) as usize;
            let n_length_extra_bits = LENGTHEXTRA[length_index] as usize;
            i += 1;
            let length_extra_bits = lz77_encoded[i as usize];
            i += 1;
            let distance_code = lz77_encoded[i as usize];
            let distance_index = distance_code as usize;
            let n_distance_extra_bits = DISTANCEEXTRA[distance_index];
            i += 1;
            let distance_extra_bits = lz77_encoded[i as usize];
            addBitsToStream(bp, out, length_extra_bits, n_length_extra_bits);
            addHuffmanSymbol(bp, out, tree_d.code(distance_code), tree_d.length(distance_code));
            addBitsToStream(bp, out, distance_extra_bits, n_distance_extra_bits as usize);
        };
        i += 1
    }
}


fn deflateDynamic(
    out: &mut ucvector,
    bp: &mut usize,
    hash: &mut Hash,
    data: &[u8],
    datapos: usize,
    dataend: usize,
    settings: &CompressSettings,
    final_: u32,
) -> Result<(), Error> {
    let mut frequencies_cl = Vec::new();
    let mut bitlen_cl = Vec::new();
    let datasize = dataend - datapos;
    let BFINAL = final_;

    let data = &data[0..dataend]; // not datasize. Truncating the length is important.
    let lz77_encoded = if settings.use_lz77 != 0 {
        encodeLZ77(hash, data, datapos, settings.windowsize, settings.minmatch, settings.nicematch, settings.lazymatching != 0)?
    } else {
        let mut t = Vec::with_capacity(datasize);
        for &d in &data[datapos..dataend] {
            t.push(d as u32);
        }
        t
    };
    let mut frequencies_ll = [0; 286];
    let mut frequencies_d = [0; 30];
    let mut l = &lz77_encoded[..];
    while !l.is_empty() {
        let symbol = l[0];
        frequencies_ll[symbol as usize] += 1;
        let skip = if symbol > 256 {
            let dist = l[2];
            frequencies_d[dist as usize] += 1;
            4
        } else {
            1
        };
        l = &l[skip..];
    }
    frequencies_ll[256] = 1;
    let tree_ll = HuffmanTree::from_frequencies(&frequencies_ll, 257, 15)?;
    let tree_d = HuffmanTree::from_frequencies(&frequencies_d, 2, 15)?;
    let numcodes_ll = tree_ll.numcodes.min(286);
    let numcodes_d = tree_d.numcodes.min(30);
    let mut bitlen_lld = Vec::new();
    for i in 0..numcodes_ll {
        bitlen_lld.push(tree_ll.length(i as u32));
    }
    for i in 0..numcodes_d {
        bitlen_lld.push(tree_d.length(i as u32));
    }
    let mut bitlen_lld_e = Vec::new();
    let mut i = 0;
    while i < bitlen_lld.len() {
        let mut j = 0;
        while i + j + 1 < (bitlen_lld.len()) && bitlen_lld[i + j + 1] == bitlen_lld[i] {
            j += 1;
        }
        if bitlen_lld[i] == 0 && j >= 2 {
            j += 1;
            if j <= 10 {
                bitlen_lld_e.push(17);
                bitlen_lld_e.push(j as u32 - 3);
            } else {
                if j > 138 {
                    j = 138;
                }
                bitlen_lld_e.push(18);
                bitlen_lld_e.push(j as u32 - 11);
            }
            i += j - 1;
        } else if j >= 3 {
            let num = j / 6;
            let rest = j % 6;
            bitlen_lld_e.push(bitlen_lld[i]);
            for _ in 0..num {
                bitlen_lld_e.push(16);
                bitlen_lld_e.push(6 - 3);
            }
            if rest >= 3 {
                bitlen_lld_e.push(16);
                bitlen_lld_e.push(rest as u32 - 3);
            } else {
                j -= rest;
            }
            i += j;
        } else {
            bitlen_lld_e.push(bitlen_lld[i]);
        }
        i += 1
    }
    frequencies_cl.resize(NUM_CODE_LENGTH_CODES, 0);
    let mut i = 0;
    while i != bitlen_lld_e.len() {
        frequencies_cl[bitlen_lld_e[i] as usize] += 1;
        /*after a repeat code come the bits that specify the number of repetitions,
                  those don't need to be in the frequencies_cl calculation*/
        if bitlen_lld_e[i] >= 16 {
            i += 1;
        } /*lenghts of code length tree is in the order as specified by deflate*/
        i += 1
    }

    let tree_cl = HuffmanTree::from_frequencies(&frequencies_cl, frequencies_cl.len(), 7)?;

    bitlen_cl.resize(tree_cl.numcodes, 0);
    for i in 0..tree_cl.numcodes {
        bitlen_cl[i] = tree_cl.length(CLCL_ORDER[i]);
    }
    while bitlen_cl[bitlen_cl.len() - 1] == 0 && bitlen_cl.len() > 4 {
        let s = bitlen_cl.len() - 1;
        bitlen_cl.resize(s, 0);
    }

    /*
        Write everything into the output

        After the BFINAL and BTYPE, the dynamic block consists out of the following:
        - 5 bits HLIT, 5 bits HDIST, 4 bits HCLEN
        - (HCLEN+4)*3 bits code lengths of code length alphabet
        - HLIT + 257 code lenghts of lit/length alphabet (encoded using the code length
          alphabet, + possible repetition codes 16, 17, 18)
        - HDIST + 1 code lengths of distance alphabet (encoded using the code length
          alphabet, + possible repetition codes 16, 17, 18)
        - compressed data
        - 256 (end code)
        */
    /*Write block type*/
    if ((*bp) & 7) == 0 {
        out.push(0); /*first bit of BTYPE "dynamic"*/
    } /*second bit of BTYPE "dynamic"*/
    *out.last_mut() |= ((BFINAL as u32) << ((*bp as u32) & 7)) as u8; /*write the HLIT, HDIST and HCLEN values*/
    (*bp) += 1; /*trim zeroes for HCLEN. HLIT and HDIST were already trimmed at tree creation*/
    /*write the code lenghts of the code length alphabet*/

    if ((*bp) & 7) == 0 {
        out.push(0); /*write the lenghts of the lit/len AND the dist alphabet*/
    } /*extra bits of repeat codes*/
    *out.last_mut() |= (0 << ((*bp as u32) & 7)) as u8; /*write the compressed data symbols*/
    (*bp) += 1; /*error: the length of the end code 256 must be larger than 0*/
    /*write the end code*/

    if ((*bp) & 7) == 0 {
        out.push(0); /*end of error-while*/
    }
    *out.last_mut() |= (1 << ((*bp as u32) & 7)) as u8;
    (*bp) += 1;

    let HLIT = (numcodes_ll - 257) as u32;
    let HDIST = (numcodes_d - 1) as u32;
    let mut HCLEN = (bitlen_cl.len() as u32) - 4;
    while bitlen_cl[HCLEN as usize + 4 - 1] == 0 && HCLEN > 0 {
        HCLEN -= 1;
    }
    addBitsToStream(bp, out, HLIT, 5);
    addBitsToStream(bp, out, HDIST, 5);
    addBitsToStream(bp, out, HCLEN, 4);
    for &b in &bitlen_cl[0..HCLEN as usize + 4] {
        addBitsToStream(bp, out, b, 3);
    }
    let mut i = 0;
    while i != bitlen_lld_e.len() {
        addHuffmanSymbol(bp, out, tree_cl.code(bitlen_lld_e[i]), tree_cl.length(bitlen_lld_e[i]));
        if bitlen_lld_e[i] == 16 {
            i += 1;
            addBitsToStream(bp, out, bitlen_lld_e[i], 2);
        } else if bitlen_lld_e[i] == 17 {
            i += 1;
            addBitsToStream(bp, out, bitlen_lld_e[i], 3);
        } else if bitlen_lld_e[i] == 18 {
            i += 1;
            addBitsToStream(bp, out, bitlen_lld_e[i], 7);
        };
        i += 1;
    }
    writeLZ77data(bp, out, &lz77_encoded, &tree_ll, &tree_d);
    if tree_ll.length(256) == 0 {
        return Err(Error(64));
    }
    addHuffmanSymbol(bp, out, tree_ll.code(256), tree_ll.length(256));
    Ok(())
}

fn deflateFixed(out: &mut ucvector, bp: &mut usize, hash: &mut Hash, data: &[u8], datapos: usize, dataend: usize, settings: &CompressSettings, final_: u32) -> Result<(), Error> {
    let BFINAL = final_;
    let tree_ll = generateFixedLitLenTree()?;
    let tree_d = generateFixedDistanceTree()?;

    if ((*bp) & 7) == 0 {
        out.push(0);
    }
    let end = out.len() - 1;
    out.slice_mut()[end] |= (BFINAL << ((*bp as u32) & 7)) as u8;
    (*bp) += 1;
    if ((*bp) & 7) == 0 {
        out.push(0);
    }
    let end = out.len() - 1;
    out.slice_mut()[end] |= (1 << ((*bp as u32) & 7)) as u8;
    (*bp) += 1;
    if ((*bp) & 7) == 0 {
        out.push(0);
    }
    let end = out.len() - 1;
    out.slice_mut()[end] |= (0 << ((*bp as u32) & 7)) as u8;
    (*bp) += 1;
    if settings.use_lz77 != 0 {
        let lz77_encoded = encodeLZ77(
            hash,
            data,
            datapos,
            settings.windowsize,
            settings.minmatch,
            settings.nicematch,
            settings.lazymatching != 0,
        )?;
        writeLZ77data(bp, out, &lz77_encoded, &tree_ll, &tree_d);
    } else {
        for &d in &data[datapos..dataend] {
            addHuffmanSymbol(bp, out, tree_ll.code(d as u32), tree_ll.length(d as u32));
        }
    }
    /*add END code*/
    addHuffmanSymbol(bp, out, tree_ll.code(256), tree_ll.length(256));
    Ok(())
}

pub(crate) fn lodepng_deflatev(inp: &[u8], settings: &CompressSettings) -> Result<ucvector, Error> {
    let mut blocksize: usize;
    let mut bp = 0;

    if settings.btype > 2 {
        return Err(Error(61));
    } else if settings.btype == 0 {
        return deflateNoCompression(inp);
    } else if settings.btype == 1 {
        blocksize = inp.len();
    } else {
        blocksize = inp.len() / 8 + 8;
        if blocksize < 65536 {
            blocksize = 65536;
        }
        if blocksize > 262144 {
            blocksize = 262144;
        };
    }
    let mut numdeflateblocks = (inp.len() + blocksize - 1) / blocksize;
    if numdeflateblocks == 0 {
        numdeflateblocks = 1;
    }
    let mut hash = Hash::new(settings.windowsize as usize)?;
    let mut out = ucvector::new();
    for i in 0..numdeflateblocks {
        let final_ = numdeflateblocks - 1 == i;
        let start = i * blocksize;
        let end = (start + blocksize).min(inp.len());
        if settings.btype == 1 {
            deflateFixed(&mut out, &mut bp, &mut hash, inp, start, end, settings, final_ as u32)?;
        } else {
            debug_assert_eq!(2, settings.btype);
            deflateDynamic(&mut out, &mut bp, &mut hash, inp, start, end, settings, final_ as u32)?;
        }
    }
    Ok(out)
}

fn deflate(inp: &[u8], settings: &CompressSettings) -> Result<ucvector, Error> {
    if let Some(cb) = settings.custom_deflate {
        unsafe {
            let mut outdata = ptr::null_mut();
            let mut outsize = 0;
            Error((cb)(&mut outdata, &mut outsize, inp.as_ptr(), inp.len(), settings)).to_result()?;
            Ok(ucvector::from_raw(&mut outdata, outsize))
        }
    } else {
        lodepng_deflatev(inp, settings)
    }
}

/* ////////////////////////////////////////////////////////////////////////// */
/* / Adler32                                                                  */
/* ////////////////////////////////////////////////////////////////////////// */
fn update_adler32(adler: u32, data: &[u8]) -> u32 {
    let mut s1 = adler & 65535;
    let mut s2 = (adler >> 16) & 65535;
    /*at least 5550 sums can be done before the sums overflow, saving a lot of module divisions*/
    for part in data.chunks(5550) {
        for &v in part {
            s1 += v as u32;
            s2 += s1;
        }
        s1 %= 65521;
        s2 %= 65521;
    }
    (s2 << 16) | s1
}

/*Return the adler32 of the bytes data[0..len-1]*/
fn adler32(data: &[u8]) -> u32 {
    update_adler32(1, data)
}


/* ////////////////////////////////////////////////////////////////////////// */
/* / Zlib                                                                   / */
/* ////////////////////////////////////////////////////////////////////////// */
pub fn lodepng_zlib_decompress(inp: &[u8], settings: &DecompressSettings) -> Result<ucvector, Error> {
    if inp.len() < 2 {
        return Err(Error(53));
    }
    /*read information from zlib header*/
    if (inp[0] as u32 * 256 + inp[1] as u32) % 31 != 0 {
        /*error: 256 * in[0] + in[1] must be a multiple of 31, the FCHECK value is supposed to be made that way*/
        return Err(Error(24));
    }
    let CM = inp[0] as u32 & 15;
    let CINFO = ((inp[0] as u32) >> 4) & 15;
    let FDICT = ((inp[1] as u32) >> 5) & 1;
    if CM != 8 || CINFO > 7 {
        /*error: only compression method 8: inflate with sliding window of 32k is supported by the PNG spec*/
        return Err(Error(25));
    }
    if FDICT != 0 {
        /*error: the specification of PNG says about the zlib stream:
              "The additional flags shall not specify a preset dictionary."*/
        return Err(Error(26));
    }
    let out = inflate(&inp[2..], settings)?;
    if settings.ignore_adler32 == 0 {
        let ADLER32 = lodepng_read32bitInt(&inp[(inp.len() - 4)..]);
        let checksum = adler32(out.slice());
        /*error, adler checksum not correct, data must be corrupted*/
        if checksum != ADLER32 {
            return Err(Error(58));
        };
    }
    Ok(out)
}

pub fn zlib_decompress(inp: &[u8], settings: &DecompressSettings) -> Result<ucvector, Error> {
    if let Some(cb) = settings.custom_zlib {
        unsafe {
            let mut outdata = ptr::null_mut();
            let mut outsize = 0;
            Error((cb)(&mut outdata, &mut outsize, inp.as_ptr(), inp.len(), settings)).to_result()?;
            Ok(ucvector::from_raw(&mut outdata, outsize))
        }
    } else {
        lodepng_zlib_decompress(inp, settings)
    }
}


pub fn lodepng_zlib_compress(outv: &mut ucvector, inp: &[u8], settings: &CompressSettings) -> Result<(), Error> {
    /*initially, *out must be NULL and outsize 0, if you just give some random *out
      that's pointing to a non allocated buffer, this'll crash*/
    /*zlib data: 1 byte CMF (CM+CINFO), 1 byte FLG, deflate data, 4 byte ADLER32 checksum of the Decompressed data*/
    let CMF = 120;
    /*0b01111000: CM 8, CINFO 7. With CINFO 7, any window size up to 32768 can be used.*/
    let FLEVEL = 0;
    let FDICT = 0;
    let mut CMFFLG = 256 * CMF + FDICT * 32 + FLEVEL * 64;
    let FCHECK = 31 - CMFFLG % 31;
    CMFFLG += FCHECK;
    /*ucvector-controlled version of the output buffer, for dynamic array*/
    outv.push((CMFFLG >> 8) as u8);
    outv.push((CMFFLG & 255) as u8);
    let deflated = deflate(inp, settings)?;
    let ADLER32 = adler32(inp);
    for &b in deflated.slice() {
        outv.push(b);
    }
    lodepng_add32bitInt(outv, ADLER32);
    Ok(())
}

/* compress using the default or custom zlib function */
pub fn zlib_compress(inp: &[u8], settings: &CompressSettings) -> Result<ucvector, Error> {
    if let Some(cb) = settings.custom_zlib {
        unsafe {
            let mut outdata = ptr::null_mut();
            let mut outsize = 0;
            Error((cb)(&mut outdata, &mut outsize, inp.as_ptr(), inp.len(), settings)).to_result()?;
            Ok(ucvector::from_raw(&mut outdata, outsize))
        }
    } else {
        let mut out = ucvector::new();
        lodepng_zlib_compress(&mut out, inp, settings)?;
        Ok(out)
    }
}

/*this is a good tradeoff between speed and compression ratio*/
pub const DEFAULT_WINDOWSIZE: usize = 2048;

/* ////////////////////////////////////////////////////////////////////////// */
/* / CRC32                                                                  / */
/* ////////////////////////////////////////////////////////////////////////// */
/* CRC polynomial: 0xedb88320 */
const LODEPNG_CRC32_TABLE: [u32; 256] = [
           0, 1996959894, 3993919788, 2567524794,  124634137, 1886057615, 3915621685, 2657392035,
   249268274, 2044508324, 3772115230, 2547177864,  162941995, 2125561021, 3887607047, 2428444049,
   498536548, 1789927666, 4089016648, 2227061214,  450548861, 1843258603, 4107580753, 2211677639,
   325883990, 1684777152, 4251122042, 2321926636,  335633487, 1661365465, 4195302755, 2366115317,
   997073096, 1281953886, 3579855332, 2724688242, 1006888145, 1258607687, 3524101629, 2768942443,
   901097722, 1119000684, 3686517206, 2898065728,  853044451, 1172266101, 3705015759, 2882616665,
   651767980, 1373503546, 3369554304, 3218104598,  565507253, 1454621731, 3485111705, 3099436303,
   671266974, 1594198024, 3322730930, 2970347812,  795835527, 1483230225, 3244367275, 3060149565,
  1994146192,   31158534, 2563907772, 4023717930, 1907459465,  112637215, 2680153253, 3904427059,
  2013776290,  251722036, 2517215374, 3775830040, 2137656763,  141376813, 2439277719, 3865271297,
  1802195444,  476864866, 2238001368, 4066508878, 1812370925,  453092731, 2181625025, 4111451223,
  1706088902,  314042704, 2344532202, 4240017532, 1658658271,  366619977, 2362670323, 4224994405,
  1303535960,  984961486, 2747007092, 3569037538, 1256170817, 1037604311, 2765210733, 3554079995,
  1131014506,  879679996, 2909243462, 3663771856, 1141124467,  855842277, 2852801631, 3708648649,
  1342533948,  654459306, 3188396048, 3373015174, 1466479909,  544179635, 3110523913, 3462522015,
  1591671054,  702138776, 2966460450, 3352799412, 1504918807,  783551873, 3082640443, 3233442989,
  3988292384, 2596254646,   62317068, 1957810842, 3939845945, 2647816111,   81470997, 1943803523,
  3814918930, 2489596804,  225274430, 2053790376, 3826175755, 2466906013,  167816743, 2097651377,
  4027552580, 2265490386,  503444072, 1762050814, 4150417245, 2154129355,  426522225, 1852507879,
  4275313526, 2312317920,  282753626, 1742555852, 4189708143, 2394877945,  397917763, 1622183637,
  3604390888, 2714866558,  953729732, 1340076626, 3518719985, 2797360999, 1068828381, 1219638859,
  3624741850, 2936675148,  906185462, 1090812512, 3747672003, 2825379669,  829329135, 1181335161,
  3412177804, 3160834842,  628085408, 1382605366, 3423369109, 3138078467,  570562233, 1426400815,
  3317316542, 2998733608,  733239954, 1555261956, 3268935591, 3050360625,  752459403, 1541320221,
  2607071920, 3965973030, 1969922972,   40735498, 2617837225, 3943577151, 1913087877,   83908371,
  2512341634, 3803740692, 2075208622,  213261112, 2463272603, 3855990285, 2094854071,  198958881,
  2262029012, 4057260610, 1759359992,  534414190, 2176718541, 4139329115, 1873836001,  414664567,
  2282248934, 4279200368, 1711684554,  285281116, 2405801727, 4167216745, 1634467795,  376229701,
  2685067896, 3608007406, 1308918612,  956543938, 2808555105, 3495958263, 1231636301, 1047427035,
  2932959818, 3654703836, 1088359270,  936918000, 2847714899, 3736837829, 1202900863,  817233897,
  3183342108, 3401237130, 1404277552,  615818150, 3134207493, 3453421203, 1423857449,  601450431,
  3009837614, 3294710456, 1567103746,  711928724, 3020668471, 3272380065, 1510334235,  755167117
];

/*Return the CRC of the bytes buf[0..len-1].*/
pub fn lodepng_crc32(data: &[u8]) -> u32 {
    let mut r = 4294967295u32;
    for &d in data {
        r = LODEPNG_CRC32_TABLE[((r ^ d as u32) & 255) as usize] ^ (r >> 8);
    }
    r ^ 4294967295
}


impl Drop for Info {
    fn drop(&mut self) {
        unsafe {
            self.clear_text();
            self.clear_itext();
            for &i in &self.unknown_chunks_data {
                lodepng_free(i as *mut _);
            }
        }
    }
}

pub fn lodepng_convert(out: &mut [u8], inp: &[u8], mode_out: &ColorMode, mode_in: &ColorMode, w: u32, h: u32) -> Result<(), Error> {
    let numpixels = w as usize * h as usize;
    if lodepng_color_mode_equal(mode_out, mode_in) {
        let numbytes = mode_in.raw_size(w, h);
        out[..numbytes].clone_from_slice(&inp[..numbytes]);
        return Ok(());
    }
    let mut tree = ColorTree::new();
    if mode_out.colortype == ColorType::PALETTE {
        let mut palette = mode_out.palette();
        let palsize = 1 << mode_out.bitdepth();
        /*if the user specified output palette but did not give the values, assume
            they want the values of the input color type (assuming that one is palette).
            Note that we never create a new palette ourselves.*/
        if palette.is_empty() {
            palette = mode_in.palette();
        }
        palette = &palette[0..palette.len().min(palsize)];
        for (i, p) in palette.iter().enumerate() {
            tree.insert((p.r, p.g, p.b, p.a), i as u16);
        }
    }
    if mode_in.bitdepth() == 16 && mode_out.bitdepth() == 16 {
        for i in 0..numpixels {
            let (r, g, b, a) = getPixelColorRGBA16(inp, i, mode_in);
            rgba16ToPixel(out, i, mode_out, r, g, b, a);
        }
    } else if mode_out.bitdepth() == 8 && mode_out.colortype == ColorType::RGBA {
        getPixelColorsRGBA8(out, numpixels as usize, true, inp, mode_in);
    } else if mode_out.bitdepth() == 8 && mode_out.colortype == ColorType::RGB {
        getPixelColorsRGBA8(out, numpixels as usize, false, inp, mode_in);
    } else {
        for i in 0..numpixels {
            let (r, g, b, a) = getPixelColorRGBA8(inp, i, mode_in);
            rgba8ToPixel(out, i, mode_out, &mut tree, r, g, b, a)?;
        }
    }
    Ok(())
}

/*out must be buffer big enough to contain full image, and in must contain the full decompressed data from
the IDAT chunks (with filter index bytes and possible padding bits)
return value is error*/
/*
  This function converts the filtered-padded-interlaced data into pure 2D image buffer with the PNG's colortype.
  Steps:
  *) if no Adam7: 1) unfilter 2) remove padding bits (= posible extra bits per scanline if bpp < 8)
  *) if adam7: 1) 7x unfilter 2) 7x remove padding bits 3) Adam7_deinterlace
  NOTE: the in buffer will be overwritten with intermediate data!
  */
fn postProcessScanlines(out: &mut [u8], inp: &mut [u8], w: usize, h: usize, info_png: &Info) -> Result<(), Error> {
    let bpp = info_png.color.bpp() as usize;
    if bpp == 0 {
        return Err(Error(31));
    }
    if info_png.interlace_method == 0 {
        if bpp < 8 && w as usize * bpp != ((w as usize * bpp + 7) / 8) * 8 {
            unfilter_aliased(inp, 0, 0, w, h, bpp)?;
            removePaddingBits(out, inp, w as usize * bpp, ((w as usize * bpp + 7) / 8) * 8, h);
        } else {
            unfilter(out, inp, w, h, bpp)?;
        };
    } else {
        let (passw, passh, filter_passstart, padded_passstart, passstart) = Adam7_getpassvalues(w, h, bpp);
        for i in 0..7 {
            unfilter_aliased(inp, padded_passstart[i], filter_passstart[i], passw[i] as usize, passh[i] as usize, bpp)?;
            if bpp < 8 {
                /*remove padding bits in scanlines; after this there still may be padding
                        bits between the different reduced images: each reduced image still starts nicely at a byte*/
                removePaddingBits_aliased(
                    inp,
                    passstart[i],
                    padded_passstart[i],
                    passw[i] as usize * bpp,
                    ((passw[i] as usize * bpp + 7) / 8) * 8,
                    passh[i] as usize,
                );
            };
        }
        Adam7_deinterlace(out, inp, w, h, bpp);
    }
    Ok(())
}

/*
  For PNG filter method 0
  this function unfilters a single image (e.g. without interlacing this is called once, with Adam7 seven times)
  out must have enough bytes allocated already, in must have the scanlines + 1 filtertype byte per scanline
  w and h are image dimensions or dimensions of reduced image, bpp is bits per pixel
  in and out are allowed to be the same memory address (but aren't the same size since in has the extra filter bytes)
  */
fn unfilter(out: &mut [u8], inp: &[u8], w: usize, h: usize, bpp: usize) -> Result<(), Error> {
    let mut prevline = None;

    /*bytewidth is used for filtering, is 1 when bpp < 8, number of bytes per pixel otherwise*/
    let bytewidth = (bpp + 7) / 8;
    let linebytes = (w * bpp + 7) / 8;
    let in_linebytes = 1 + linebytes; /*the extra filterbyte added to each row*/

    for (out_line, in_line) in out.chunks_mut(linebytes).zip(inp.chunks(in_linebytes)).take(h) {
        let filterType = in_line[0];
        unfilterScanline(out_line, &in_line[1..], prevline, bytewidth, filterType, linebytes)?;
        prevline = Some(out_line);
    }
    Ok(())
}

fn unfilter_aliased(inout: &mut [u8], out_off: usize, in_off: usize, w: usize, h: usize, bpp: usize) -> Result<(), Error> {
    let mut prevline = None;
    /*bytewidth is used for filtering, is 1 when bpp < 8, number of bytes per pixel otherwise*/
    let bytewidth = (bpp + 7) / 8;
    let linebytes = (w * bpp + 7) / 8;
    for y in 0..h as usize {
        let outindex = linebytes * y;
        let inindex = (1 + linebytes) * y; /*the extra filterbyte added to each row*/
        let filterType = inout[in_off + inindex];
        unfilterScanline_aliased(inout, out_off + outindex, in_off + inindex + 1, prevline, bytewidth, filterType, linebytes)?;
        prevline = Some(out_off + outindex);
    }
    Ok(())
}

/*
  For PNG filter method 0
  unfilter a PNG image scanline by scanline. when the pixels are smaller than 1 byte,
  the filter works byte per byte (bytewidth = 1)
  precon is the previous unfiltered scanline, recon the result, scanline the current one
  the incoming scanlines do NOT include the filtertype byte, that one is given in the parameter filterType instead
  recon and scanline MAY be the same memory address! precon must be disjoint.
  */
fn unfilterScanline(recon: &mut [u8], scanline: &[u8], precon: Option<&[u8]>, bytewidth: usize, filterType: u8, length: usize) -> Result<(), Error> {
    match filterType {
        0 => recon.clone_from_slice(scanline),
        1 => {
            recon[0..bytewidth].clone_from_slice(&scanline[0..bytewidth]);
            for i in bytewidth..length {
                recon[i] = scanline[i].wrapping_add(recon[i - bytewidth]);
            }
        },
        2 => if let Some(precon) = precon {
            for i in 0..length {
                recon[i] = scanline[i].wrapping_add(precon[i]);
            }
        } else {
            recon.clone_from_slice(scanline);
        },
        3 => if let Some(precon) = precon {
            for i in 0..bytewidth {
                recon[i] = scanline[i].wrapping_add(precon[i] >> 1);
            }
            for i in bytewidth..length {
                let t = recon[i - bytewidth] as u16 + precon[i] as u16;
                recon[i] = scanline[i].wrapping_add((t as u8) >> 1);
            }
        } else {
            recon[0..bytewidth].clone_from_slice(&scanline[0..bytewidth]);
            for i in bytewidth..length {
                recon[i] = scanline[i].wrapping_add(recon[i - bytewidth] >> 1);
            }
        },
        4 => if let Some(precon) = precon {
            for i in 0..bytewidth {
                recon[i] = scanline[i].wrapping_add(precon[i]);
            }
            for i in bytewidth..length {
                recon[i] = scanline[i].wrapping_add(paethPredictor(
                    recon[i - bytewidth] as i16,
                    precon[i] as i16,
                    precon[i - bytewidth] as i16,
                ));
            }
        } else {
            recon[0..bytewidth].clone_from_slice(&scanline[0..bytewidth]);
            for i in bytewidth..length {
                recon[i] = scanline[i].wrapping_add(recon[i - bytewidth]);
            }
        },
        _ => return Err(Error(36)),
    }
    Ok(())
}

fn unfilterScanline_aliased(inout: &mut [u8], recon: usize, scanline: usize, precon: Option<usize>, bytewidth: usize, filterType: u8, length: usize) -> Result<(), Error> {
    match filterType {
        0 => for i in 0..length {
            inout[recon + i] = inout[scanline + i];
        },
        1 => {
            for i in 0..bytewidth {
                inout[recon + i] = inout[scanline + i];
            }
            for i in bytewidth..length {
                inout[recon + i] = inout[scanline + i].wrapping_add(inout[recon + i - bytewidth]);
            }
        },
        2 => if let Some(precon) = precon {
            for i in 0..length {
                inout[recon + i] = inout[scanline + i].wrapping_add(inout[precon + i]);
            }
        } else {
            for i in 0..length {
                inout[recon + i] = inout[scanline + i];
            }
        },
        3 => if let Some(precon) = precon {
            for i in 0..bytewidth {
                inout[recon + i] = inout[scanline + i].wrapping_add(inout[precon + i] >> 1);
            }
            for i in bytewidth..length {
                let t = inout[recon + i - bytewidth] as u16 + inout[precon + i] as u16;
                inout[recon + i] = inout[scanline + i].wrapping_add((t >> 1) as u8);
            }
        } else {
            for i in 0..bytewidth {
                inout[recon + i] = inout[scanline + i];
            }
            for i in bytewidth..length {
                inout[recon + i] = inout[scanline + i].wrapping_add(inout[recon + i - bytewidth] >> 1);
            }
        },
        4 => if let Some(precon) = precon {
            for i in 0..bytewidth {
                inout[recon + i] = inout[scanline + i].wrapping_add(inout[precon + i]);
            }
            for i in bytewidth..length {
                inout[recon + i] = inout[scanline + i].wrapping_add(paethPredictor(
                    inout[recon + i - bytewidth] as i16,
                    inout[precon + i] as i16,
                    inout[precon + i - bytewidth] as i16,
                ));
            }
        } else {
            for i in 0..bytewidth {
                inout[recon + i] = inout[scanline + i];
            }
            for i in bytewidth..length {
                inout[recon + i] = inout[scanline + i].wrapping_add(inout[recon + i - bytewidth]);
            }
        },
        _ => return Err(Error(36)),
    }
    Ok(())
}

/*
  After filtering there are still padding bits if scanlines have non multiple of 8 bit amounts. They need
  to be removed (except at last scanline of (Adam7-reduced) image) before working with pure image buffers
  for the Adam7 code, the color convert code and the output to the user.
  in and out are allowed to be the same buffer, in may also be higher but still overlapping; in must
  have >= ilinebits*h bits, out must have >= olinebits*h bits, olinebits must be <= ilinebits
  also used to move bits after earlier such operations happened, e.g. in a sequence of reduced images from Adam7
  only useful if (ilinebits - olinebits) is a value in the range 1..7
  */
fn removePaddingBits(out: &mut [u8], inp: &[u8], olinebits: usize, ilinebits: usize, h: usize) {
    let diff = ilinebits - olinebits; /*input and output bit pointers*/
    let mut ibp = 0;
    let mut obp = 0;
    for _ in 0..h {
        for _ in 0..olinebits {
            let bit = readBitFromReversedStream(&mut ibp, inp);
            setBitOfReversedStream(&mut obp, out, bit);
        }
        ibp += diff;
    }
}

fn removePaddingBits_aliased(inout: &mut [u8], out_off: usize, in_off: usize, olinebits: usize, ilinebits: usize, h: usize) {
    let diff = ilinebits - olinebits; /*input and output bit pointers*/
    let mut ibp = 0;
    let mut obp = 0;
    for _ in 0..h {
        for _ in 0..olinebits {
            let bit = readBitFromReversedStream(&mut ibp, &inout[in_off..]);
            setBitOfReversedStream(&mut obp, &mut inout[out_off..], bit);
        }
        ibp += diff;
    }
}

/*
in: non-interlaced image with size w*h
out: the same pixels, but re-ordered according to PNG's Adam7 interlacing, with
 no padding bits between scanlines, but between reduced images so that each
 reduced image starts at a byte.
bpp: bits per pixel
there are no padding bits, not between scanlines, not between reduced images
in has the following size in bits: w * h * bpp.
out is possibly bigger due to padding bits between reduced images
NOTE: comments about padding bits are only relevant if bpp < 8
*/
fn Adam7_interlace(out: &mut [u8], inp: &[u8], w: usize, h: usize, bpp: usize) {
    let (passw, passh, _, _, passstart) = Adam7_getpassvalues(w, h, bpp);
    let bpp = bpp;
    if bpp >= 8 {
        for i in 0..7 {
            let bytewidth = bpp / 8;
            for y in 0..passh[i] as usize {
                for x in 0..passw[i] as usize {
                    let pixelinstart = ((ADAM7_IY[i] as usize + y * ADAM7_DY[i] as usize) * w as usize + ADAM7_IX[i] as usize + x * ADAM7_DX[i] as usize) * bytewidth;
                    let pixeloutstart = passstart[i] + (y * passw[i] as usize + x) * bytewidth;
                    out[pixeloutstart..(bytewidth + pixeloutstart)]
                        .clone_from_slice(&inp[pixelinstart..(bytewidth + pixelinstart)]);
                }
            }
        }
    } else {
        for i in 0..7 {
            let ilinebits = bpp * passw[i] as usize;
            let olinebits = bpp * w;
            for y in 0..passh[i] as usize {
                for x in 0..passw[i] as usize {
                    let mut ibp = (ADAM7_IY[i] as usize + y * ADAM7_DY[i] as usize) * olinebits + (ADAM7_IX[i] as usize + x * ADAM7_DX[i] as usize) * bpp;
                    let mut obp = (8 * passstart[i]) + (y * ilinebits + x * bpp);
                    for _ in 0..bpp {
                        let bit = readBitFromReversedStream(&mut ibp, inp);
                        setBitOfReversedStream(&mut obp, out, bit);
                    }
                }
            }
        }
    };
}

/* ////////////////////////////////////////////////////////////////////////// */
/* / PNG Decoder                                                            / */
/* ////////////////////////////////////////////////////////////////////////// */
/*read the information from the header and store it in the Info. return value is error*/
pub fn lodepng_inspect(decoder: &DecoderSettings, inp: &[u8]) -> Result<(Info, usize, usize), Error> {
    if inp.len() < 33 {
        /*error: the data length is smaller than the length of a PNG header*/
        return Err(Error(27));
    }
    /*when decoding a new PNG image, make sure all parameters created after previous decoding are reset*/
    let mut info_png = Info::new();
    if &inp[0..8] != &[137, 80, 78, 71, 13, 10, 26, 10] {
        /*error: the first 8 bytes are not the correct PNG signature*/
        return Err(Error(28));
    }
    if lodepng_chunk_length(&inp[8..]) != 13 {
        /*error: header size must be 13 bytes*/
        return Err(Error(94));
    }
    if lodepng_chunk_type(&inp[8..]) != b"IHDR" {
        /*error: it doesn't start with a IHDR chunk!*/
        return Err(Error(29));
    }
    /*read the values given in the header*/
    let w = lodepng_read32bitInt(&inp[16..]) as usize;
    let h = lodepng_read32bitInt(&inp[20..]) as usize;
    let bitdepth = inp[24];
    if bitdepth == 0 || bitdepth > 16 {
        return Err(Error(29));
    }
    info_png.color.set_bitdepth(inp[24] as u32);
    info_png.color.colortype = match inp[25] {
        0 => ColorType::GREY,
        2 => ColorType::RGB,
        3 => ColorType::PALETTE,
        4 => ColorType::GREY_ALPHA,
        6 => ColorType::RGBA,
        _ => return Err(Error(31)),
    };
    info_png.compression_method = inp[26] as u32;
    info_png.filter_method = inp[27] as u32;
    info_png.interlace_method = inp[28] as u32;
    if w == 0 || h == 0 {
        return Err(Error(93));
    }
    if decoder.ignore_crc == 0 {
        let CRC = lodepng_read32bitInt(&inp[29..]);
        let checksum = lodepng_crc32(&inp[12..(12 + 17)]);
        if CRC != checksum {
            return Err(Error(57));
        };
    }
    if info_png.compression_method != 0 {
        /*error: only compression method 0 is allowed in the specification*/
        return Err(Error(32));
    }
    if info_png.filter_method != 0 {
        /*error: only filter method 0 is allowed in the specification*/
        return Err(Error(33));
    }
    if info_png.interlace_method > 1 {
        /*error: only interlace methods 0 and 1 exist in the specification*/
        return Err(Error(34));
    }
    checkPngColorValidity(info_png.color.colortype, info_png.color.bitdepth())?;
    Ok((info_png, w, h))
}

/*read a PNG, the result will be in the same color type as the PNG (hence "generic")*/
fn decodeGeneric(state: &mut State, inp: &[u8]) -> Result<(ucvector, usize, usize), Error> {
    let mut IEND = 0u8; /*the data from idat chunks*/
    /*for unknown chunk order*/
    let mut unknown = false;
    let mut critical_pos = ChunkPosition::IHDR;
    /*provide some proper output values if error will happen*/
    let (info, w, h) = lodepng_inspect(&state.decoder, inp)?;
    state.info_png = info;

    /*reads header and resets other parameters in state->info_png*/
    let numpixels = match w.checked_mul(h) {
        Some(n) => n,
        None => {
            return Err(Error(92));
        },
    };
    /*multiplication overflow possible further below. Allows up to 2^31-1 pixel
      bytes with 16-bit RGBA, the rest is room for filter bytes.*/
    if numpixels > 268435455 {
        return Err(Error(92)); /*first byte of the first chunk after the header*/
    }
    let mut idat = Vec::new();
    let mut chunk = &inp[33..];
    /*loop through the chunks, ignoring unknown chunks and stopping at IEND chunk.
      IDAT data is put at the start of the in buffer*/
    while IEND == 0 {
        if chunk.len() < 12 {
            return Err(Error(30));
        }
        /*length of the data of the chunk, excluding the length bytes, chunk type and CRC bytes*/
        let data = lodepng_chunk_data(chunk)?;
        match lodepng_chunk_type(chunk) {
            b"IDAT" => {
                idat.extend_from_slice(data);
                critical_pos = ChunkPosition::IDAT;
            },
            b"IEND" => {
                IEND = 1u8;
            },
            b"PLTE" => {
                readChunk_PLTE(&mut state.info_png.color, data)?;
                critical_pos = ChunkPosition::PLTE;
            },
            b"tRNS" => {
                readChunk_tRNS(&mut state.info_png.color, data)?;
            },
            b"bKGD" => {
                readChunk_bKGD(&mut state.info_png, data)?;
            },
            b"tEXt" => if state.decoder.read_text_chunks != 0 {
                readChunk_tEXt(&mut state.info_png, data)?;
            },
            b"zTXt" => if state.decoder.read_text_chunks != 0 {
                readChunk_zTXt(&mut state.info_png, &state.decoder.zlibsettings, data)?;
            },
            b"iTXt" => if state.decoder.read_text_chunks != 0 {
                readChunk_iTXt(&mut state.info_png, &state.decoder.zlibsettings, data)?;
            },
            b"tIME" => {
                readChunk_tIME(&mut state.info_png, data)?;
            },
            b"pHYs" => {
                readChunk_pHYs(&mut state.info_png, data)?;
            },
            _ => {
                if !lodepng_chunk_ancillary(chunk) {
                    return Err(Error(69));
                }
                unknown = true;
                if state.decoder.remember_unknown_chunks != 0 {
                    state.info_png.push_unknown_chunk(critical_pos, chunk)?;
                }
            },
        };
        if state.decoder.ignore_crc == 0 && !unknown && !lodepng_chunk_check_crc(chunk) {
            return Err(Error(57));
        }
        if IEND == 0 {
            chunk = lodepng_chunk_next(chunk);
        };
    }
    /*predict output size, to allocate exact size for output buffer to avoid more dynamic allocation.
      If the decompressed size does not match the prediction, the image must be corrupt.*/
    let predict = if state.info_png.interlace_method == 0 {
        /*The extra *h is added because this are the filter bytes every scanline starts with*/
        state.info_png.color.raw_size_idat(w, h) + h
    } else {
        /*Adam-7 interlaced: predicted size is the sum of the 7 sub-images sizes*/
        let color = &state.info_png.color;
        let mut predict = color.raw_size_idat((w + 7) >> 3, (h + 7) >> 3) + ((h + 7) >> 3) as usize;
        if w > 4 {
            predict += color.raw_size_idat((w + 3) >> 3, (h + 7) >> 3) + ((h + 7) >> 3) as usize;
        }
        predict += color.raw_size_idat((w + 3) >> 2, (h + 3) >> 3) + ((h + 3) >> 3) as usize;
        if w > 2 {
            predict += color.raw_size_idat((w + 1) >> 2, (h + 3) >> 2) + ((h + 3) >> 2) as usize;
        }
        predict += color.raw_size_idat((w + 1) >> 1, (h + 1) >> 2) + ((h + 1) >> 2) as usize;
        if w > 1 {
            predict += color.raw_size_idat((w + 0) >> 1, (h + 1) >> 1) + ((h + 1) >> 1) as usize;
        }
        predict += color.raw_size_idat((w + 0), (h + 0) >> 1) + ((h + 0) >> 1) as usize;
        predict
    };
    let mut scanlines = zlib_decompress(&idat, &state.decoder.zlibsettings)?;
    if scanlines.len() != predict {
        /*decompressed size doesn't match prediction*/
        return Err(Error(91));
    }
    let mut out = ucvector::new();
    out.resize(state.info_png.color.raw_size(w as u32, h as u32))?;
    for i in out.slice_mut() {
        *i = 0;
    }
    postProcessScanlines(out.slice_mut(), scanlines.slice_mut(), w, h, &state.info_png)?;
    Ok((out, w, h))
}


pub fn lodepng_decode(state: &mut State, inp: &[u8]) -> Result<(ucvector, usize, usize), Error> {
    let (decoded, w, h) = decodeGeneric(state, inp)?;

    if state.decoder.color_convert == 0 || lodepng_color_mode_equal(&state.info_raw, &state.info_png.color) {
        /*store the info_png color settings on the info_raw so that the info_raw still reflects what colortype
            the raw image has to the end user*/
        if state.decoder.color_convert == 0 {
            /*color conversion needed; sort of copy of the data*/
            state.info_raw = state.info_png.color.clone();
        }
        Ok((decoded, w, h))
    } else {
        /*TODO: check if this works according to the statement in the documentation: "The converter can convert
            from greyscale input color type, to 8-bit greyscale or greyscale with alpha"*/
        if !(state.info_raw.colortype == ColorType::RGB || state.info_raw.colortype == ColorType::RGBA) && (state.info_raw.bitdepth() != 8) {
            return Err(Error(56)); /*unsupported color mode conversion*/
        }
        let mut out = ucvector::new();
        out.resize(state.info_raw.raw_size(w as u32, h as u32))?;
        lodepng_convert(out.slice_mut(), decoded.slice(), &state.info_raw, &state.info_png.color, w as u32, h as u32)?;
        Ok((out, w, h))
    }
}


pub fn lodepng_decode_memory(inp: &[u8], colortype: ColorType, bitdepth: u32) -> Result<(ucvector, usize, usize), Error> {
    let mut state = State::new();
    state.info_raw.colortype = colortype;
    state.info_raw.set_bitdepth(bitdepth);
    lodepng_decode(&mut state, inp)
}

pub fn lodepng_decode_file(filename: &Path, colortype: ColorType, bitdepth: u32) -> Result<(ucvector, usize, usize), Error> {
    let buf = lodepng_load_file(filename)?;
    lodepng_decode_memory(buf.slice(), colortype, bitdepth)
}



/* load file into buffer that already has the correct allocated size. Returns error code.*/
pub fn lodepng_buffer_file(out: &mut [u8], filename: &Path) -> Result<(), Error> {
    fs::File::open(filename)
        .and_then(|mut f| f.read_exact(out))
        .map_err(|_| Error(78))?;
    Ok(())
}

pub fn lodepng_load_file(filename: &Path) -> Result<ucvector, Error> {
    let size = match fs::metadata(filename) {
        Ok(m) => m.len() as usize,
        Err(_) => return Err(Error(83)),
    };
    let mut v = ucvector::new();
    v.resize(size)?;
    lodepng_buffer_file(v.slice_mut(), filename)?;
    Ok(v)
}

/*write given buffer to the file, overwriting the file, it doesn't append to it.*/
pub fn lodepng_save_file(buffer: &[u8], filename: &Path) -> Result<(), Error> {
    fs::File::create(filename)
        .and_then(|mut f| f.write_all(buffer))
        .map_err(|_| Error(79))
}

fn addUnknownChunks(out: &mut ucvector, mut inchunk: &[u8]) -> Result<(), Error> {
    while !inchunk.is_empty() {
        chunk_append(out, inchunk)?;
        inchunk = lodepng_chunk_next(inchunk);
    }
    Ok(())
}

pub const LODEPNG_VERSION_STRING: &'static [u8] = b"20161127\0";

pub fn lodepng_encode(image: &[u8], w: u32, h: u32, state: &mut State) -> Result<ucvector, Error> {
    let w = w as usize;
    let h = h as usize;

    let mut info = state.info_png.clone();
    if (info.color.colortype == ColorType::PALETTE || state.encoder.force_palette != 0) && (info.color.palette().is_empty() || info.color.palette().len() > 256) {
        return Err(Error(68));
    }
    if state.encoder.auto_convert != 0 {
        /*write signature and chunks*/
        info.color = auto_choose_color(image, w, h, &state.info_raw)?;
    }
    if state.encoder.zlibsettings.btype > 2 {
        /*bKGD (must come between PLTE and the IDAt chunks*/
        return Err(Error(61)); /*PLTE*/
    } /*pHYs (must come before the IDAT chunks)*/
    if state.info_png.interlace_method > 1 {
        return Err(Error(71)); /*unknown chunks between PLTE and IDAT*/
        /*IDAT (multiple IDAT chunks must be consecutive)*/
    }
    checkPngColorValidity(info.color.colortype, info.color.bitdepth())?; /*tEXt and/or zTXt */
    checkLodeColorValidity(state.info_raw.colortype, state.info_raw.bitdepth())?; /*LodePNG version id in text chunk */

    let data = if !lodepng_color_mode_equal(&state.info_raw, &info.color) {
        let size = (w * h * (info.color.bpp() as usize) + 7) / 8;
        let mut converted = vec![0u8; size];
        lodepng_convert(&mut converted, image, &info.color, &state.info_raw, w as u32, h as u32)?;
        preProcessScanlines(&converted, w, h, &info, &state.encoder)?
    } else {
        preProcessScanlines(image, w, h, &info, &state.encoder)?
    };

    let mut outv = ucvector::new();
    writeSignature(&mut outv);

    addChunk_IHDR(&mut outv, w, h, info.color.colortype, info.color.bitdepth() as usize, info.interlace_method as u8)?;
    if let Some(chunks) = info.unknown_chunks_data(ChunkPosition::IHDR) {
        addUnknownChunks(&mut outv, chunks)?;
    }
    if info.color.colortype == ColorType::PALETTE {
        addChunk_PLTE(&mut outv, &info.color)?;
    }
    if state.encoder.force_palette != 0 && (info.color.colortype == ColorType::RGB || info.color.colortype == ColorType::RGBA) {
        addChunk_PLTE(&mut outv, &info.color)?;
    }
    if info.color.colortype == ColorType::PALETTE && getPaletteTranslucency(info.color.palette()) != PaletteTranslucency::Opaque {
        addChunk_tRNS(&mut outv, &info.color)?;
    }
    if (info.color.colortype == ColorType::GREY || info.color.colortype == ColorType::RGB) && info.color.key().is_some() {
        addChunk_tRNS(&mut outv, &info.color)?;
    }
    if info.background_defined != 0 {
        addChunk_bKGD(&mut outv, &info)?;
    }
    if info.phys_defined != 0 {
        addChunk_pHYs(&mut outv, &info)?;
    }
    if let Some(chunks) = info.unknown_chunks_data(ChunkPosition::PLTE) {
        addUnknownChunks(&mut outv, chunks)?;
    }
    addChunk_IDAT(&mut outv, &data, &state.encoder.zlibsettings)?;
    if info.time_defined != 0 {
        addChunk_tIME(&mut outv, &info.time)?;
    }
    for (t, v) in info.text_keys_cstr() {
        if t.to_bytes().len() > 79 {
            return Err(Error(66));
        }
        if t.to_bytes().len() < 1 {
            return Err(Error(67));
        }
        if state.encoder.text_compression != 0 {
            addChunk_zTXt(&mut outv, t, v, &state.encoder.zlibsettings)?;
        } else {
            addChunk_tEXt(&mut outv, t, v)?;
        }
    }
    if state.encoder.add_id != 0 {
        let alread_added_id_text = info.text_keys_cstr()
            .any(|(t, _)| t.to_str().unwrap_or("") == "LodePNG");
        if !alread_added_id_text {
            /*it's shorter as tEXt than as zTXt chunk*/
            let l = CStr::from_bytes_with_nul(b"LodePNG\0").unwrap();
            let v = CStr::from_bytes_with_nul(LODEPNG_VERSION_STRING).unwrap();
            addChunk_tEXt(&mut outv, l, v)?;
        }
    }
    for (k, l, t, s) in info.itext_keys() {
        if k.as_bytes().len() > 79 {
            return Err(Error(66));
        }
        if k.as_bytes().len() < 1 {
            return Err(Error(67));
        }
        addChunk_iTXt(&mut outv, state.encoder.text_compression != 0, k, l, t, s, &state.encoder.zlibsettings)?;
    }
    if let Some(chunks) = info.unknown_chunks_data(ChunkPosition::IDAT) {
        addUnknownChunks(&mut outv, chunks)?;
    }
    addChunk_IEND(&mut outv)?;
    Ok(outv)
}

/*profile must already have been inited with mode.
It's ok to set some parameters of profile to done already.*/
pub fn get_color_profile(inp: &[u8], w: u32, h: u32, mode: &ColorMode) -> Result<ColorProfile, Error> {
    let mut profile = ColorProfile::new();
    let numpixels: usize = w as usize * h as usize;
    let mut colored_done = mode.is_greyscale_type();
    let mut alpha_done = !mode.can_have_alpha();
    let mut numcolors_done = false;
    let bpp = mode.bpp() as usize;
    let mut bits_done = bpp == 1;
    let maxnumcolors = match bpp {
        1 => 2,
        2 => 4,
        4 => 16,
        5...8 => 256,
        _ => 257,
    };

    /*Check if the 16-bit input is truly 16-bit*/
    let mut sixteen = false;
    if mode.bitdepth() == 16 {
        for i in 0..numpixels {
            let (r, g, b, a) = getPixelColorRGBA16(inp, i, mode);
            if (r & 255) != ((r >> 8) & 255) || (g & 255) != ((g >> 8) & 255) || (b & 255) != ((b >> 8) & 255) || (a & 255) != ((a >> 8) & 255) {
                /*first and second byte differ*/
                sixteen = true;
                break;
            };
        }
    }
    if sixteen {
        profile.bits = 16;
        bits_done = true;
        numcolors_done = true;
        /*counting colors no longer useful, palette doesn't support 16-bit*/
        for i in 0..numpixels {
            let (r, g, b, a) = getPixelColorRGBA16(inp, i, mode);
            if !colored_done && (r != g || r != b) {
                profile.colored = 1;
                colored_done = true;
            }
            if !alpha_done {
                let matchkey = r == profile.key_r && g == profile.key_g && b == profile.key_b;
                if a != 65535 && (a != 0 || (profile.key != 0 && !matchkey)) {
                    profile.alpha = 1;
                    profile.key = 0;
                    alpha_done = true;
                } else if a == 0 && profile.alpha == 0 && profile.key == 0 {
                    profile.key = 1;
                    profile.key_r = r;
                    profile.key_g = g;
                    profile.key_b = b;
                } else if a == 65535 && profile.key != 0 && matchkey {
                    profile.alpha = 1;
                    profile.key = 0;
                    alpha_done = true;
                };
            }
            if alpha_done && numcolors_done && colored_done && bits_done {
                break;
            };
        }
        if profile.key != 0 && profile.alpha == 0 {
            for i in 0..numpixels {
                let (r, g, b, a) = getPixelColorRGBA16(inp, i, mode);
                if a != 0 && r == profile.key_r && g == profile.key_g && b == profile.key_b {
                    profile.alpha = 1;
                    profile.key = 0;
                }
            }
        }
    } else {
        let mut tree = ColorTree::new();
        for i in 0..numpixels {
            let (r, g, b, a) = getPixelColorRGBA8(inp, i, mode);
            if !bits_done && profile.bits < 8 {
                let bits = getValueRequiredBits(r) as u32;
                if bits > profile.bits {
                    profile.bits = bits;
                };
            }
            bits_done = profile.bits as usize >= bpp;
            if !colored_done && (r != g || r != b) {
                profile.colored = 1;
                colored_done = true;
                if profile.bits < 8 {
                    profile.bits = 8;
                };
                /*PNG has no colored modes with less than 8-bit per channel*/
            }
            if !alpha_done {
                let matchkey = r as u16 == profile.key_r && g as u16 == profile.key_g && b as u16 == profile.key_b;
                if a != 255 && (a != 0 || (profile.key != 0 && !matchkey)) {
                    profile.alpha = 1;
                    profile.key = 0;
                    alpha_done = true;
                    if profile.bits < 8 {
                        profile.bits = 8;
                    };
                /*PNG has no alphachannel modes with less than 8-bit per channel*/
                } else if a == 0 && profile.alpha == 0 && profile.key == 0 {
                    profile.key = 1;
                    profile.key_r = r as u16;
                    profile.key_g = g as u16;
                    profile.key_b = b as u16;
                } else if a == 255 && profile.key != 0 && matchkey {
                    profile.alpha = 1;
                    profile.key = 0;
                    alpha_done = true;
                    if profile.bits < 8 {
                        profile.bits = 8;
                    };
                    /*PNG has no alphachannel modes with less than 8-bit per channel*/
                };
            }
            if !numcolors_done && tree.get(&(r, g, b, a)).is_none() {
                tree.insert((r, g, b, a), profile.numcolors as u16);
                if profile.numcolors < 256 {
                    profile.palette[profile.numcolors as usize] = RGBA { r, g, b, a };
                }
                profile.numcolors += 1;
                numcolors_done = profile.numcolors >= maxnumcolors;
            }
            if alpha_done && numcolors_done && colored_done && bits_done {
                break;
            };
        }
        if profile.key != 0 && profile.alpha == 0 {
            for i in 0..numpixels {
                let (r, g, b, a) = getPixelColorRGBA8(inp, i, mode);
                if a != 0 && r as u16 == profile.key_r && g as u16 == profile.key_g && b as u16 == profile.key_b {
                    profile.alpha = 1;
                    profile.key = 0;
                    /*PNG has no alphachannel modes with less than 8-bit per channel*/
                    if profile.bits < 8 {
                        profile.bits = 8;
                    };
                };
            }
        }
        /*make the profile's key always 16-bit for consistency - repeat each byte twice*/
        profile.key_r += profile.key_r << 8;
        profile.key_g += profile.key_g << 8;
        profile.key_b += profile.key_b << 8;
    }
    Ok(profile)
}

/*Automatically chooses color type that gives smallest amount of bits in the
output image, e.g. grey if there are only greyscale pixels, palette if there
are less than 256 colors, ...
Updates values of mode with a potentially smaller color model. mode_out should
contain the user chosen color model, but will be overwritten with the new chosen one.*/
pub fn auto_choose_color(image: &[u8], w: usize, h: usize, mode_in: &ColorMode) -> Result<ColorMode, Error> {
    let mut mode_out = ColorMode::new();
    let mut prof = get_color_profile(image, w as u32, h as u32, mode_in)?;

    mode_out.clear_key();
    if prof.key != 0 && w * h <= 16 {
        prof.alpha = 1;
        prof.key = 0;
        /*PNG has no alphachannel modes with less than 8-bit per channel*/
        if prof.bits < 8 {
            prof.bits = 8;
        };
    }
    let n = prof.numcolors;
    let palettebits = if n <= 2 {
        1
    } else if n <= 4 {
        2
    } else if n <= 16 {
        4
    } else {
        8
    };
    let palette_ok = (n <= 256 && prof.bits <= 8) &&
        (w * h >= (n * 2) as usize) &&
        (prof.colored != 0 || prof.bits > palettebits);
    if palette_ok {
        let pal = &prof.palette[0..prof.numcolors as usize];
        /*remove potential earlier palette*/
        mode_out.palette_clear();
        for p in pal {
            mode_out.palette_add(*p)?;
        }
        mode_out.colortype = ColorType::PALETTE;
        mode_out.set_bitdepth(palettebits);
        if mode_in.colortype == ColorType::PALETTE && mode_in.palette().len() >= mode_out.palette().len() && mode_in.bitdepth() == mode_out.bitdepth() {
            /*If input should have same palette colors, keep original to preserve its order and prevent conversion*/
            mode_out = mode_in.clone();
        };
    } else {
        mode_out.set_bitdepth(prof.bits);
        mode_out.colortype = if prof.alpha != 0 {
            if prof.colored != 0 {
                ColorType::RGBA
            } else {
                ColorType::GREY_ALPHA
            }
        } else if prof.colored != 0 {
            ColorType::RGB
        } else {
            ColorType::GREY
        };
        if prof.key != 0 {
            let mask = ((1 << mode_out.bitdepth()) - 1) as u16;
            /*profile always uses 16-bit, mask converts it*/
            mode_out.set_key(
                prof.key_r as u16 & mask,
                prof.key_g as u16 & mask,
                prof.key_b as u16 & mask);
        };
    }
    Ok(mode_out)
}

pub fn lodepng_filesize(filename: &Path) -> Option<u64> {
    fs::metadata(filename).map(|m| m.len()).ok()
}

pub fn lodepng_encode_memory(image: &[u8], w: u32, h: u32, colortype: ColorType, bitdepth: u32) -> Result<ucvector, Error> {
    let mut state = State::new();
    state.info_raw.colortype = colortype;
    state.info_raw.set_bitdepth(bitdepth);
    state.info_png.color.colortype = colortype;
    state.info_png.color.set_bitdepth(bitdepth);
    lodepng_encode(image, w, h, &mut state)
}

impl EncoderSettings {
    unsafe fn predefined_filters(&self, len: usize) -> Result<&[u8], Error> {
        if self.predefined_filters.is_null() {
            Err(Error(1))
        } else {
            Ok(slice::from_raw_parts(self.predefined_filters, len))
        }
    }
}

impl ColorProfile {
    pub fn new() -> Self {
        Self {
            colored: 0,
            key: 0,
            key_r: 0,
            key_g: 0,
            key_b: 0,
            alpha: 0,
            numcolors: 0,
            bits: 1,
            palette: [RGBA{r:0,g:0,b:0,a:0}; 256],
        }
    }
}

/*Returns how many bits needed to represent given value (max 8 bit)*/
fn getValueRequiredBits(value: u8) -> u8 {
    match value {
        0 | 255 => 1,
        x if x % 17 == 0 => {
            /*The scaling of 2-bit and 4-bit values uses multiples of 85 and 17*/
            if value % 85 == 0 { 2 } else { 4 }
        },
        _ => 8,
    }
}

#[inline]
fn longest_match(a: &[u8], b: &[u8]) -> usize {
    let len = a.len().min(b.len());
    let a = &a[0..len];
    let b = &b[0..len];
    for i in 0..len {
        if a[i] != b[i] {
            return i;
        }
    }
    return len;
}

/*
LZ77-encode the data. Return value is error code. The input are raw bytes, the output
is in the form of unsigned integers with codes representing for example literal bytes, or
length/distance pairs.
It uses a hash table technique to let it encode faster. When doing LZ77 encoding, a
sliding window (of windowsize) is used, and all past bytes in that window can be used as
the "dictionary". A brute force search through all possible distances would be slow, and
this hash technique is one out of several ways to speed this up.
*/
fn encodeLZ77(
    hash: &mut Hash,
    in_: &[u8],
    inpos: usize,
    windowsize: u32,
    minmatch: u32,
    nicematch: u32,
    lazymatching: bool,
) -> Result<Vec<u32>, Error> {
    let mut out = Vec::new();
    let nicematch = (nicematch).min(MAX_SUPPORTED_DEFLATE_LENGTH as u32);
    /*for large window lengths, assume the user wants no compression loss. Otherwise, max hash chain length speedup.*/
    let maxchainlength = if windowsize >= 8192 {
        windowsize
    } else {
        windowsize / 8
    };
    let maxlazymatch = if windowsize >= 8192 {
        MAX_SUPPORTED_DEFLATE_LENGTH as u32
    } else {
        64
    };
    if windowsize == 0 || windowsize > 32768 {
        Error(60).to_result()?;
    }
    /*error: windowsize smaller/larger than allowed*/
    if (windowsize & (windowsize - 1)) != 0 {
        Error(90).to_result()?; /*error: must be power of two*/
    }
    let mut numzeros = 0;
    let mut lazylength = 0;
    let mut lazyoffset = 0;
    let mut lazy = false;
    let mut pos = inpos;
    while pos < in_.len() {
        let mut wpos = pos as u32 & (windowsize - 1);
        let hashval = getHash(in_, pos) as u32;
        let usezeros = true;
        /*not sure if setting it to false for windowsize < 8192 is better or worse*/
        if usezeros && hashval == 0 {
            if numzeros == 0 {
                numzeros = countZeros(in_, pos); /*the length and offset found for the current position*/
            } else if pos + numzeros as usize > in_.len() || in_[pos + numzeros as usize - 1] != 0 {
                numzeros -= 1; /*search for the longest string*/
            };
        } else {
            numzeros = 0;
        }
        updateHashChain(hash, wpos, hashval, numzeros as u16);
        let mut length = 0;
        let mut offset: u32 = 0;
        let mut hashpos = hash.chain[wpos as usize] as u32;
        let lastptr = in_.len().min(pos + MAX_SUPPORTED_DEFLATE_LENGTH) as u32;
        let mut prev_offset = 0;
        for _ in 0..maxchainlength {
            let current_offset = if hashpos <= wpos {
                wpos - hashpos
            } else {
                wpos + windowsize - hashpos
            };
            if current_offset < prev_offset {
                break;
            }
            /*stop when went completely around the circular buffer*/
            prev_offset = current_offset; /*test the next characters*/
            if current_offset > 0 {
                let mut foreptr = pos as u32; /*common case in PNGs is lots of zeros. Quickly skip over them as a speedup*/
                let mut backptr = pos as u32 - current_offset;
                if numzeros >= 3 {
                    let mut skip = hash.zeros[hashpos as usize] as u32;
                    if skip > numzeros {
                        skip = numzeros;
                    }
                    backptr += skip;
                    foreptr += skip;
                }
                let current_length = longest_match(&in_[foreptr as usize..lastptr as usize], &in_[backptr as usize..]) as u32;
                if current_length > length {
                    length = current_length;
                    offset = current_offset;
                    /*jump out once a length of max length is found (speed gain). This also jumps
                              out if length is MAX_SUPPORTED_DEFLATE_LENGTH*/
                    if current_length >= nicematch {
                        break;
                    };
                }
            }
            if hashpos == hash.chain[hashpos as usize] as u32 {
                break;
            }
            if numzeros >= 3 && length > numzeros {
                hashpos = hash.chainz[hashpos as usize] as u32;
                if hash.zeros[hashpos as usize] as u32 != numzeros {
                    break;
                };
            } else {
                hashpos = hash.chain[hashpos as usize] as u32;
                /*outdated hash value, happens if particular value was not encountered in whole last window*/
                if hash.val[hashpos as usize] as u32 != hashval {
                    break; /*try the next byte*/
                }; /*push the previous character as literal*/
            };
        }
        if lazymatching {
            if !lazy && length >= 3 && length <= maxlazymatch && length < MAX_SUPPORTED_DEFLATE_LENGTH as u32 {
                lazy = true;
                lazylength = length;
                lazyoffset = offset;
                pos += 1;
                continue;
            }
            if lazy {
                lazy = false;
                if pos == 0 {
                    return Err(Error(81));
                }
                if length > lazylength + 1 {
                    out.push(in_[pos - 1] as u32);
                } else {
                    length = lazylength;
                    offset = lazyoffset;
                    hash.head[hashval as usize] = -1;
                    /*the same hashchain update will be done, this ensures no wrong alteration*/
                    hash.headz[numzeros as usize] = -1; /*idem*/
                    pos -= 1; /*too big (or overflown negative) offset*/
                }; /*encode it as length/distance pair or literal value*/
            }; /*only lengths of 3 or higher are supported as length/distance pair*/
        }
        if length >= 3 && offset > windowsize {
            return Err(Error(86));
        }
        if length < 3 || length < minmatch || (length == 3 && offset > 4096) {
            /*compensate for the fact that longer offsets have more extra bits, a
                  length of only 3 may be not worth it then*/
            out.push(in_[pos] as u32);
        } else {
            addLengthDistance(&mut out, length, offset);
            for _ in 1..length {
                pos += 1;
                wpos = pos as u32 & (windowsize - 1);
                let hashval = getHash(in_, pos) as u32;
                if usezeros && hashval == 0 {
                    if numzeros == 0 {
                        numzeros = countZeros(in_, pos);
                    } else if pos + numzeros as usize > in_.len() || in_[pos + numzeros as usize - 1] != 0 {
                        numzeros -= 1;
                    }
                } else {
                    numzeros = 0;
                }
                updateHashChain(hash, wpos, hashval, numzeros as u16);
            }
        }
        pos += 1;
    }
    Ok(out)
}


unsafe impl Sync for CompressSettings {}
unsafe impl Sync for DecompressSettings {}
