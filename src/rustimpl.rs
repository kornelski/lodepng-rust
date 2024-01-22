
use crate::*;
use crate::ffi::ColorProfile;
use crate::ffi::IntlText;
use crate::ffi::LatinText;
use crate::ffi::State;
use crate::ChunkPosition;
use crate::zlib;
use std::num::NonZeroU8;

pub use rgb::RGBA8 as RGBA;
use rgb::ComponentSlice;
use rgb::RGBA16;
use std::cell::Cell;
use std::collections::HashMap;
use std::io::prelude::*;
use std::io;

use std::slice;

/*8 bytes PNG signature, aka the magic bytes*/
fn write_signature(out: &mut Vec<u8>) {
    out.push(137u8);
    out.push(80u8);
    out.push(78u8);
    out.push(71u8);
    out.push(13u8);
    out.push(10u8);
    out.push(26u8);
    out.push(10u8);
}

#[derive(Eq, PartialEq)]
enum PaletteTranslucency {
    Opaque,
    Key,
    Semi,
}

/*
palette must have 4 * palettesize bytes allocated, and given in format RGBARGBARGBARGBA…
returns 0 if the palette is opaque,
returns 1 if the palette has a single color with alpha 0 ==> color key
returns 2 if the palette is semi-translucent.
*/
fn get_palette_translucency(palette: &[RGBA]) -> PaletteTranslucency {
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

/*The opposite of the remove_padding_bits function
  olinebits must be >= ilinebits*/
#[inline(never)]
fn add_padding_bits_line(out: &mut [u8], inp: &[u8], olinebits: usize, ilinebits: usize, y: u32) {
    let iline = y as usize * ilinebits;
    for i in 0..ilinebits {
        let bit = read_bit_from_reversed_stream(iline + i, inp);
        set_bit_of_reversed_stream(i, out, bit);
    }
    for i in ilinebits..olinebits {
        set_bit_of_reversed_stream(i, out, 0);
    }
}

#[inline]
fn linebits_exact(w: u32, bpp: NonZeroU8) -> usize {
    w as usize * bpp.get() as usize
}

#[inline]
fn linebits_rounded(w: u32, bpp: NonZeroU8) -> usize {
    linebytes_rounded(w, bpp) * 8
}

#[inline]
fn linebytes_rounded(w: u32, bpp: NonZeroU8) -> usize {
    (w as usize * bpp.get() as usize + 7) / 8
}

fn filtered_scanlines(out: &mut dyn Write, inp: &[u8], w: u32, h: u32, info_png: &Info, settings: &EncoderSettings) -> Result<(), Error> {
    if info_png.interlace_method == 0 {
        filter(out, inp, w, h, &info_png.color, settings)?;
    } else {
        let bpp = info_png.color.bpp_();
        let passes = adam7_pass_values(w, h, bpp);
        /*image size plus an extra byte per scanline + possible padding bits*/
        let mut adam7 = zero_vec(passes.clone().map(|l| l.packed_len).sum::<usize>() + 1)?;
        adam7_interlace(&mut adam7, inp, w, h, bpp);
        let mut adam7 = &mut adam7[..];
        for pass in passes {
            if pass.w == 0 {
                continue;
            }
            filter(out, adam7, pass.w, pass.h, &info_png.color, settings)?;
            adam7 = &mut adam7[pass.packed_len..];
        }
    }
    Ok(())
}

/*
  For PNG filter method 0
  out must be a buffer with as size: h + (w * h * bpp + 7) / 8, because there are
  the scanlines with 1 extra byte per scanline
  */
#[inline(never)]
fn filter(out: &mut dyn Write, inp: &[u8], w: u32, h: u32, info: &ColorMode, settings: &EncoderSettings) -> Result<(), Error> {
    debug_assert!(w != 0);
    debug_assert!(h != 0);
    let bpp = info.bpp_();
    /*the width of a scanline in bytes, not including the filter type*/
    let linebytes = linebytes_rounded(w, bpp);
    if bpp.get() > 4*16 || linebytes == 0 {
        return Err(Error::new(31));
    }
    let mut f = make_filter(w, h, info, settings)?;
    let mut out_buffer = zero_vec(1 + linebytes)?;
    if bpp.get() < 8 && linebits_exact(w, bpp) != linebits_rounded(w, bpp) {
        let mut lines_tmp = zero_vec(linebytes * 2)?;
        let (mut tmp, mut tmp_prev) = lines_tmp.split_at_mut(linebytes);
        for y in 0..h {
            std::mem::swap(&mut tmp, &mut tmp_prev);
            add_padding_bits_line(&mut tmp[..], inp, linebits_rounded(w, bpp), linebits_exact(w, bpp), y);
            f(&mut out_buffer, tmp, if y > 0 { Some(tmp_prev) } else { None });
            out.write_all(&out_buffer)?;
        }
    } else {
        let mut prevline = None;
        // interlace gives larger buffers
        let inp = inp.get(..h as usize * linebytes).ok_or(Error::new(31))?;
        for inp in inp.chunks_exact(linebytes) {
            f(&mut out_buffer, inp, prevline);
            prevline = Some(inp);
            out.write_all(&out_buffer)?;
        }
    }
    Ok(())
}

#[inline(never)]
fn make_filter<'a>(w: u32, h: u32, info: &ColorMode, settings: &'a EncoderSettings) -> Result<Box<dyn FnMut(&mut [u8], &[u8], Option<&[u8]>) + 'a>, Error> {
    let bpp = info.bpp_();
    debug_assert!(w != 0);
    /*bytewidth is used for filtering, is 1 when bpp < 8, number of bytes per pixel otherwise*/
    let bytewidth = (bpp.get() + 7) / 8;

    /*the width of a scanline in bytes, not including the filter type*/
    let linebytes = linebytes_rounded(w, bpp);
    debug_assert!(linebytes > 0);

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
    let strategy = if settings.filter_palette_zero && (info.colortype == ColorType::PALETTE || info.bitdepth() < 8) {
        FilterStrategy::ZERO
    } else {
        settings.filter_strategy
    };
    Ok(match strategy {
        FilterStrategy::ZERO => {
            Box::new(move |out, inp, _prevline| {
                debug_assert_eq!(out.len(), 1+linebytes);
                debug_assert_eq!(inp.len(), linebytes);

                let Some((f, line)) = out.split_first_mut() else { return; };
                *f = 0;
                if line.len() == inp.len() {
                    line.copy_from_slice(inp);
                }
            })
        },
        FilterStrategy::MINSUM => {
            let mut best = zero_vec(1 + linebytes)?;
            let mut attempt = zero_vec(1 + linebytes)?;
            Box::new(move |out, inp, prevline| {
                let mut smallest = 0;
                for type_ in 0..5 {
                    let Some((f, line)) = attempt.split_first_mut() else { return; };
                    *f = type_;
                    filter_scanline(line, inp, prevline, bytewidth, type_);
                    let sum = if type_ == 0 {
                        attempt.iter().map(|&s| s as usize).sum()
                    } else {
                        /*For differences, each byte should be treated as signed, values above 127 are negative
                          (converted to signed char). filter_type 0 isn't a difference though, so use unsigned there.
                          This means filter_type 0 is almost never chosen, but that is justified.*/
                        attempt.iter().map(|&s| if s < 128 { s } else { 255 - s } as usize).sum()
                    };
                    /*check if this is smallest sum (or if type == 0 it's the first case so always store the values)*/
                    if type_ == 0 || sum < smallest {
                        smallest = sum;
                        std::mem::swap(&mut attempt, &mut best);
                    }
                }
                out.copy_from_slice(&best);
            })
        },
        FilterStrategy::ENTROPY => {
            let mut best = zero_vec(1 + linebytes)?;
            let mut attempt = zero_vec(1 + linebytes)?;
            Box::new(move |out, inp, prevline| {
                let mut smallest = 0.;
                for type_ in 0..5 {
                    let Some((f, line)) = attempt.split_first_mut() else { return; };
                    *f = type_;
                    filter_scanline(line, inp, prevline, bytewidth, type_);
                    let sum = entropy(&attempt);
                    /*check if this is smallest sum (or if type == 0 it's the first case so always store the values)*/
                    if type_ == 0 || sum < smallest {
                        smallest = sum;
                        std::mem::swap(&mut attempt, &mut best);
                    };
                }
                out.copy_from_slice(&best);
            })
        },
        FilterStrategy::PREDEFINED => {
            let mut filters = unsafe { settings.predefined_filters(h as usize)? }.iter().copied();
            Box::new(move |out, inp, prevline| {
                let type_ = filters.next().unwrap_or(0);
                let Some((f, line)) = out.split_first_mut() else { return; };
                *f = type_;
                filter_scanline(line, inp, prevline, bytewidth, type_);
            })
        },
        FilterStrategy::BRUTE_FORCE => {
            /*brute force filter chooser.
            deflate the scanline after every filter attempt to see which one deflates best.
            This is very slow and gives only slightly smaller, sometimes even larger, result*/
            let mut best = zero_vec(1 + linebytes)?;
            let mut attempt = zero_vec(1 + linebytes)?;
            let mut prev_line_best = zero_vec(1 + linebytes)?;
            let mut gz = zlib::Estimator::new(linebytes);
            Box::new(move |out, inp, prevline| {
                let mut smallest = 0;
                for type_ in 0..5 {
                    let Some((f, line)) = attempt.split_first_mut() else { return; };
                    *f = type_;
                    filter_scanline(line, inp, prevline, bytewidth, type_);
                    let size = gz.estimate_compressed_size(&attempt, &prev_line_best);
                    /*check if this is smallest size (or if type == 0 it's the first case so always store the values)*/
                    if type_ == 0 || size < smallest {
                        smallest = size;
                        std::mem::swap(&mut attempt, &mut best);
                    }
                }
                out.copy_from_slice(&best);
                std::mem::swap(&mut prev_line_best, &mut best);
            })
        },
    })
}

fn entropy(attempt: &[u8]) -> f32 {
    let mut count = [0u32; 256];
    for byte in attempt.iter().copied() {
        count[byte as usize] += 1;
    }
    count.iter().copied().filter(|&c| c > 0).map(|c| {
        let p = c as f32;
        (1. / p).log2() * p
    }).sum::<f32>()
}

#[test]
fn test_filter_b1() {
    test_filter_inner(1);
}

#[test]
fn test_filter_b2() {
    test_filter_inner(2);
}

#[test]
fn test_filter_b3() {
    test_filter_inner(3);
}

#[test]
fn test_filter_b4() {
    test_filter_inner(4);
}

#[test]
fn test_filter_b8() {
    test_filter_inner(8);
}

#[cfg(test)]
fn test_filter_with_data(line1: &[u8], line2: &[u8], bytewidth: u8) {
    let mut line3 = Vec::new();

    for line_width in [1,2,3,4,5,6,7,8,9,15,16,31,32,128,line1.len()/bytewidth as usize] {
        let line1 = &line1[..line_width * bytewidth as usize];
        let line2 = &line2[..line_width * bytewidth as usize];
        let mut filtered = vec![99u8; line_width * bytewidth as usize];
        let mut unfiltered = vec![66u8; line_width * bytewidth as usize];
        for filter_type in 0..5 {
            filter_scanline(&mut filtered, line1, Some(line2), bytewidth, filter_type);
            if filter_type == 1 || filter_type == 3 {
                if filter_type == 1 {
                    unfilter_scanline_filter_1(&mut unfiltered, &filtered, bytewidth).unwrap();
                } else {
                    unfilter_scanline_filter_3(&mut unfiltered, &filtered, line2, bytewidth).unwrap();
                }
                assert_eq!(line1, &unfiltered, "width={line_width}, prev+filter={filter_type}, bytewidth={bytewidth}");
            }

            for offset in [bytewidth as usize, bytewidth as usize+1, bytewidth as usize*2] {
                line3.clear(); line3.resize(offset, 0);
                line3.extend_from_slice(&filtered);
                unfilter_scanline_aliased(&mut line3, offset, Some(line2), bytewidth, filter_type, line1.len());
                assert_eq!(line1, &line3[..line1.len()], "aliased width={line_width}, prev+filter={filter_type}, bytewidth={bytewidth}");
            }
        }
        for filter_type in 0..5 {
            filter_scanline(&mut filtered, line1, None, bytewidth, filter_type);
            for offset in [bytewidth as usize, bytewidth as usize+1, bytewidth as usize*2] {
                line3.clear(); line3.resize(offset, 0);
                line3.extend_from_slice(&filtered);
                unfilter_scanline_aliased(&mut line3, offset, None, bytewidth, filter_type, line1.len());
                assert_eq!(line1, &line3[..line1.len()], "aliased width={line_width}, none+filter={filter_type}, bytewidth={bytewidth}");
            }
        }
    }
}

#[cfg(test)]
fn test_filter_inner(bytewidth: u8) {
    let mut line1 = Vec::with_capacity(1 << 16);
    let mut line2 = Vec::with_capacity(1 << 16);
    for p in 0..256 {
        for q in 0..256 {
            line1.push(q as u8);
            line2.push(p as u8);
        }
    }
    test_filter_with_data(&line1, &line2, bytewidth);

    let mut n = 0u64;
    line1.iter_mut().for_each(|px| {
        n = n.wrapping_add(13) ^ n.wrapping_mul(*px as _);
        *px ^= n as u8;
    });
    line2.iter_mut().for_each(|px| {
        n = n.wrapping_add(*px as _) ^ n.wrapping_mul(7);
        *px ^= n as u8;
    });
    test_filter_with_data(&line1, &line2, bytewidth);
}

#[inline(never)]
fn filter_scanline(out: &mut [u8], scanline: &[u8], prevline: Option<&[u8]>, bytewidth: u8, filter_type: u8) {
    let bytewidth = bytewidth as usize;
    let length = out.len();
    // help the optimizer remove bounds checks
    if bytewidth > 8 || bytewidth < 1 || length > u32::MAX as usize || length != scanline.len() || prevline.map_or(false, move |p| p.len() != length) || bytewidth > length {
        debug_assert!(false);
        return;
    }
    match filter_type {
        0 => {
            out.copy_from_slice(scanline);
        },
        1 => {
            let (out_start, out) = out.split_at_mut(bytewidth);
            let (scanline_start, scanline_next) = scanline.split_at(bytewidth);
            out_start.copy_from_slice(scanline_start);

            for (out, (s_next, s_prev)) in out.iter_mut().zip(scanline_next.iter().copied().zip(scanline.iter().copied())) {
                *out = s_next.wrapping_sub(s_prev);
            }
        },
        2 => if let Some(prevline) = prevline {
            if prevline.len() != length { return }

            for (out, (s, prev)) in out.iter_mut().zip(scanline.iter().copied().zip(prevline.iter().copied())) {
                *out = s.wrapping_sub(prev);
            }
        } else {
            out.copy_from_slice(scanline);
        },
        3 => if let Some(prevline) = prevline {
            if prevline.len() != length { return }

            let (out_start, out) = out.split_at_mut(bytewidth);
            let (prevline_start, prevline_next) = prevline.split_at(bytewidth);
            let (scanline_start, scanline_next) = scanline.split_at(bytewidth);

            for (out, (prev, scan)) in out_start.iter_mut().zip(prevline_start.iter().copied().zip(scanline_start.iter().copied())) {
                *out = scan.wrapping_sub(prev >> 1);
            }

            for (out, ((p_next, s_prev), s_next)) in out.iter_mut().zip(prevline_next.iter().copied().zip(scanline.iter().copied()).zip(scanline_next.iter().copied())) {
                let sum = s_prev as u16 + p_next as u16;
                *out = s_next.wrapping_sub((sum >> 1) as u8);
            }
        } else {
            let (out_start, out) = out.split_at_mut(bytewidth);
            let (scanline_start, scanline_next) = scanline.split_at(bytewidth);
            out_start.copy_from_slice(scanline_start);

            for (out, (s_next, s_prev)) in out.iter_mut().zip(scanline_next.iter().copied().zip(scanline.iter().copied())) {
                *out = s_next.wrapping_sub(s_prev >> 1);
            }
        },
        4 => if let Some(prevline) = prevline {
            if prevline.len() != length { return }

            let (out_start, out) = out.split_at_mut(bytewidth);
            let (scanline_start, scanline_next) = scanline.split_at(bytewidth);
            let (prevline_start, prevline_next) = prevline.split_at(bytewidth);
            out_start.copy_from_slice(scanline_start);

            for (out, (s, p)) in out_start.iter_mut().zip(scanline_start.iter().copied().zip(prevline_start.iter().copied())) {
                *out = s.wrapping_sub(p);
            }

            let s = scanline.iter().copied().zip(scanline_next.iter().copied());
            let p = prevline.iter().copied().zip(prevline_next.iter().copied());

            for (out, ((s, s_next), (p, p_next))) in out.iter_mut().zip(s.zip(p)) {
                let pred = paeth_predictor(s, p_next, p);
                *out = s_next.wrapping_sub(pred);
            }
        } else {
            let (out_start, out) = out.split_at_mut(bytewidth);
            let (scanline_start, scanline_next) = scanline.split_at(bytewidth);
            out_start.copy_from_slice(scanline_start);

            for (out, (s_next, s_prev)) in out.iter_mut().zip(scanline_next.iter().copied().zip(scanline.iter().copied())) {
                *out = s_next.wrapping_sub(s_prev);
            }
        },
        _ => {
            debug_assert!(false);
        },
    };
}

#[inline]
fn paeth_predictor(a: u8, b: u8, c: u8) -> u8 {
    let a = a as i16;
    let b = b as i16;
    let c = c as i16;
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

impl Info {
    /// It's supposed to be in UTF-8, but trusting chunk data to be valid would be naive
    pub(crate) fn push_itext(&mut self, key: &[u8], langtag: &[u8], transkey: &[u8], value: &[u8]) -> Result<(), Error> {
        self.itexts.push(IntlText {
            key: String::from_utf8_lossy(key).into_owned().into(),
            langtag: String::from_utf8_lossy(langtag).into_owned().into(),
            transkey: String::from_utf8_lossy(transkey).into_owned().into(),
            value: String::from_utf8_lossy(value).into_owned().into(),
        });
        Ok(())
    }

    pub(crate) fn push_text(&mut self, k: &[u8], v: &[u8]) -> Result<(), Error> {
        self.texts.push(LatinText {
            key: k.into(),
            value: v.into(),
        });
        Ok(())
    }

    fn push_unknown_chunk(&mut self, critical_pos: ChunkPosition, chunk: &[u8]) -> Result<(), Error> {
        self.unknown_chunks[critical_pos as usize].try_reserve(chunk.len())?;
        self.unknown_chunks[critical_pos as usize].extend_from_slice(chunk);
        Ok(())
    }
}

fn add_color_bits(out: &mut [u8], index: usize, bits: u32, mut inp: u32) {
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

type ColorIndices = HashMap<RGBA, u8>;

#[inline(always)]
fn rgba8_to_pixel(out: &mut [u8], i: usize, mode: &ColorMode, colormap: &mut ColorIndices, /*for palette*/ px: RGBA) -> Result<(), Error> {
    match mode.colortype {
        ColorType::GREY => {
            let grey = px.r; /*((unsigned short)r + g + b) / 3*/
            if mode.bitdepth == 8 {
                out[i] = grey; /*take the most significant bits of grey*/
            } else if mode.bitdepth == 16 {
                out[i * 2 + 0] = {
                    out[i * 2 + 1] = grey; /*color not in palette*/
                    out[i * 2 + 1]
                }; /*((unsigned short)r + g + b) / 3*/
            } else {
                let grey = (grey >> (8 - mode.bitdepth)) & ((1 << mode.bitdepth) - 1); /*no error*/
                add_color_bits(out, i, mode.bitdepth, grey.into());
            };
        },
        ColorType::RGB => if mode.bitdepth == 8 {
            out[i * 3 + 0] = px.r;
            out[i * 3 + 1] = px.g;
            out[i * 3 + 2] = px.b;
        } else {
            debug_assert_eq!(16, mode.bitdepth);
            out[i * 6 + 0] = px.r;
            out[i * 6 + 1] = px.r;
            out[i * 6 + 2] = px.g;
            out[i * 6 + 3] = px.g;
            out[i * 6 + 4] = px.b;
            out[i * 6 + 5] = px.b;
        },
        ColorType::PALETTE => {
            let index = *colormap.get(&px).ok_or(Error::new(82))?;
            if mode.bitdepth >= 8 {
                out[i] = index;
            } else {
                add_color_bits(out, i, mode.bitdepth, u32::from(index));
            };
        },
        ColorType::GREY_ALPHA => {
            let grey = px.r;
            if mode.bitdepth == 8 {
                out[i * 2 + 0] = grey;
                out[i * 2 + 1] = px.a;
            } else {
                debug_assert_eq!(16, mode.bitdepth);
                out[i * 4 + 0] = grey;
                out[i * 4 + 1] = grey;
                out[i * 4 + 2] = px.a;
                out[i * 4 + 3] = px.a;
            }
        },
        ColorType::RGBA => if mode.bitdepth == 8 {
            out[i * 4 + 0] = px.r;
            out[i * 4 + 1] = px.g;
            out[i * 4 + 2] = px.b;
            out[i * 4 + 3] = px.a;
        } else {
            debug_assert_eq!(16, mode.bitdepth);
            out[i * 8 + 0] = px.r;
            out[i * 8 + 1] = px.r;
            out[i * 8 + 2] = px.g;
            out[i * 8 + 3] = px.g;
            out[i * 8 + 4] = px.b;
            out[i * 8 + 5] = px.b;
            out[i * 8 + 6] = px.a;
            out[i * 8 + 7] = px.a;
        },
        ColorType::BGRA |
        ColorType::BGR |
        ColorType::BGRX => {
            return Err(Error::new(31));
        },
    };
    Ok(())
}

/*put a pixel, given its RGBA16 color, into image of any color 16-bitdepth type*/
#[inline(always)]
fn rgba16_to_pixel(out: &mut [u8], mode: &ColorMode, px: RGBA16) {
    match mode.colortype {
        ColorType::GREY => {
            if out.len() < 2 { debug_assert!(false); return; }
            let grey = px.r;
            out[0] = (grey >> 8) as u8;
            out[1] = grey as u8;
        },
        ColorType::RGB => {
            if out.len() < 6 { debug_assert!(false); return; }
            out[0] = (px.r >> 8) as u8;
            out[1] = px.r as u8;
            out[2] = (px.g >> 8) as u8;
            out[3] = px.g as u8;
            out[4] = (px.b >> 8) as u8;
            out[5] = px.b as u8;
        },
        ColorType::GREY_ALPHA => {
            if out.len() < 4 { debug_assert!(false); return; }
            let grey = px.r;
            out[0] = (grey >> 8) as u8;
            out[1] = grey as u8;
            out[2] = (px.a >> 8) as u8;
            out[3] = px.a as u8;
        },
        ColorType::RGBA => {
            if out.len() < 8 { debug_assert!(false); return; }
            out[0] = (px.r >> 8) as u8;
            out[1] = px.r as u8;
            out[2] = (px.g >> 8) as u8;
            out[3] = px.g as u8;
            out[4] = (px.b >> 8) as u8;
            out[5] = px.b as u8;
            out[6] = (px.a >> 8) as u8;
            out[7] = px.a as u8;
        },
        ColorType::BGR |
        ColorType::BGRA |
        ColorType::BGRX |
        ColorType::PALETTE => {
            debug_assert!(false);
        },
    }
}

fn get_pixel_low_bpp(inp: &[u8], i: usize, mode: &ColorMode) -> u8 {
    if mode.bitdepth() == 8 {
        inp[i]
    } else {
        let j = i * mode.bitdepth() as usize;
        read_bits_from_reversed_stream(j, inp, mode.bitdepth() as usize) as u8
    }
}

/*Get RGBA8 color of pixel with index i (y * width + x) from the raw image with given color type.*/
#[inline]
fn get_pixel_color_rgba8(px: &[u8], mode: &ColorMode) -> RGBA {
    match mode.colortype {
        ColorType::RGB => if mode.bitdepth() == 8 {
            let px = &px[..3];
            let r = px[0];
            let g = px[1];
            let b = px[2];
            let a = if mode.key() == Some((u16::from(r), u16::from(g), u16::from(b))) {
                0
            } else {
                255
            };
            RGBA::new(r, g, b, a)
        } else {
            debug_assert_eq!(6, mode.bpp()/8);
            let px = &px[..6];
            RGBA::new(
                px[0],
                px[2],
                px[4],
                if mode.key()
                    == Some((
                        256 * px[0] as u16 + px[1] as u16,
                        256 * px[2] as u16 + px[3] as u16,
                        256 * px[4] as u16 + px[5] as u16,
                    )) {
                    0
                } else {
                    255
                },
            )
        },
        ColorType::GREY => {
            if mode.bitdepth() == 8 {
                let t = px[0];
                let a = if mode.key() == Some((u16::from(t), u16::from(t), u16::from(t))) {
                    0
                } else {
                    255
                };
                RGBA::new(t, t, t, a)
            } else {
                debug_assert_eq!(16, mode.bitdepth);
                let px = &px[..2]; // reduces bounds checks
                let t = px[0];
                let g = 256 * px[0] as u16 + px[1] as u16;
                let a = if mode.key() == Some((g, g, g)) {
                    0
                } else {
                    255
                };
                RGBA::new(t, t, t, a)
            }
        },
        ColorType::GREY_ALPHA => if mode.bitdepth == 8 {
            let px = &px[..2];
            let t = px[0];
            RGBA::new(t, t, t, px[1])
        } else {
            debug_assert_eq!(16, mode.bitdepth);
            let px = &px[..4];
            let t = px[0];
            RGBA::new(t, t, t, px[2])
        },
        ColorType::RGBA => if mode.bitdepth() == 8 {
            let px = &px[..4];
            RGBA::new(px[0], px[1], px[2], px[3])
        } else {
            debug_assert_eq!(16, mode.bitdepth);
            let px = &px[..8];
            RGBA::new(px[0], px[2], px[4], px[6])
        },
        ColorType::BGRA => {
            let px = &px[..4];
            RGBA::new(px[2], px[1], px[0], px[3])
        },
        ColorType::BGR => {
            let px = &px[..3];
            let b = px[0];
            let g = px[1];
            let r = px[2];
            let a = if mode.key() == Some((u16::from(r), u16::from(g), u16::from(b))) {
                0
            } else {
                255
            };
            RGBA::new(r, g, b, a)
        },
        ColorType::BGRX => {
            let px = &px[..3];
            let b = px[0];
            let g = px[1];
            let r = px[2];
            let a = if mode.key() == Some((u16::from(r), u16::from(g), u16::from(b))) {
                0
            } else {
                255
            };
            RGBA::new(r, g, b, a)
        },
        ColorType::PALETTE => {
            unreachable!()
        },
    }
}
/*Similar to get_pixel_color_rgba8, but with all the for loops inside of the color
mode test cases, optimized to convert the colors much faster, when converting
to RGBA or RGB with 8 bit per cannel. buffer must be RGBA or RGB output with
enough memory, if has_alpha is true the output is RGBA. mode has the color mode
of the input buffer.*/
#[inline(never)]
fn get_pixel_colors_rgba8(buffer: &mut [u8], has_alpha: bool, inp: &[u8], mode: &ColorMode) {
    let num_channels = if has_alpha { 4 } else { 3 };
    let key = mode.key();
    let has_key = key.is_some();
    match mode.colortype {
        ColorType::GREY => {
            if mode.bitdepth() == 8 {
                for (buffer, inp) in buffer.chunks_exact_mut(num_channels).zip(inp.iter().copied()) {
                    buffer[..3].fill(inp);
                    if has_alpha {
                        let a = inp as u16;
                        buffer[3] = if has_key && key == Some((a, a, a)) {
                            0
                        } else {
                            255
                        };
                    }
                }
            } else if mode.bitdepth() == 16 {
                for (buffer, inp) in buffer.chunks_exact_mut(num_channels).zip(inp.chunks_exact(2)) {
                    buffer[..3].fill(inp[0]);
                    if has_alpha {
                        let a = 256 * inp[0] as u16 + inp[1] as u16;
                        buffer[3] = if has_key && key == Some((a, a, a)) {
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
                for buffer in buffer.chunks_exact_mut(num_channels) {
                    let nbits = mode.bitdepth() as usize;
                    let value = read_bits_from_reversed_stream(j, inp, nbits); j += nbits;
                    buffer[0] = ((value * 255) / highest) as u8;
                    buffer[1] = ((value * 255) / highest) as u8;
                    buffer[2] = ((value * 255) / highest) as u8;
                    if has_alpha {
                        let a = value as u16;
                        buffer[3] = if has_key && key == Some((a, a, a)) {
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
                for (buffer, inp) in buffer.chunks_exact_mut(num_channels).zip(inp.chunks_exact(3)) {
                    buffer[..3].copy_from_slice(inp);
                    if has_alpha {
                        buffer[3] = if has_key && key == Some((inp[0] as u16, inp[1] as u16, inp[2] as u16)) {
                            0
                        } else {
                            255
                        };
                    };
                }
            } else {
                debug_assert_eq!(16, mode.bitdepth);
                for (buffer, inp) in buffer.chunks_exact_mut(num_channels).zip(inp.chunks_exact(6)) {
                    buffer[0] = inp[0];
                    buffer[1] = inp[2];
                    buffer[2] = inp[4];
                    if has_alpha {
                        let r = 256 * inp[0] as u16 + inp[1] as u16;
                        let g = 256 * inp[2] as u16 + inp[3] as u16;
                        let b = 256 * inp[4] as u16 + inp[5] as u16;
                        buffer[3] = if has_key && key == Some((r, g, b)) {
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
            let pal = mode.palette();
            for (i, buffer) in buffer.chunks_exact_mut(num_channels).enumerate() {
                let index = if mode.bitdepth >= 8 {
                    inp[i] as usize
                } else {
                    let nbits = mode.bitdepth() as usize;
                    let res = read_bits_from_reversed_stream(j, inp, nbits);
                    j += nbits;
                    res as usize
                };
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
                    let p = pal[index];
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
            for (buffer, inp) in buffer.chunks_exact_mut(num_channels).zip(inp.chunks_exact(2)) {
                buffer[..3].fill(inp[0]);
                if has_alpha {
                    buffer[3] = inp[1];
                };
            }
        } else {
            debug_assert_eq!(16, mode.bitdepth);
            for (buffer, inp) in buffer.chunks_exact_mut(num_channels).zip(inp.chunks_exact(4)) {
                buffer[..3].fill(inp[0]);
                if has_alpha {
                    buffer[3] = inp[2];
                };
            }
        },
        ColorType::RGBA => if mode.bitdepth() == 8 {
            for (buffer, inp) in buffer.chunks_exact_mut(num_channels).zip(inp.chunks_exact(4)) {
                buffer[0..3].copy_from_slice(&inp[..3]);
                if has_alpha {
                    buffer[3] = inp[3];
                }
            }
        } else {
            debug_assert_eq!(16, mode.bitdepth);
            for (buffer, inp) in buffer.chunks_exact_mut(num_channels).zip(inp.chunks_exact(8)) {
                buffer[0] = inp[0];
                buffer[1] = inp[2];
                buffer[2] = inp[4];
                if has_alpha {
                    buffer[3] = inp[6];
                }
            }
        },
        ColorType::BGR => {
            for (buffer, inp) in buffer.chunks_exact_mut(num_channels).zip(inp.chunks_exact(3)) {
                buffer[0] = inp[2];
                buffer[1] = inp[1];
                buffer[2] = inp[0];
                if has_alpha {
                    buffer[3] = if has_key && key == Some((buffer[0] as u16, buffer[1] as u16, buffer[2] as u16)) {
                        0
                    } else {
                        255
                    };
                };
            }
        },
        ColorType::BGRX => {
            for (buffer, inp) in buffer.chunks_exact_mut(num_channels).zip(inp.chunks_exact(4)) {
                buffer[0] = inp[2];
                buffer[1] = inp[1];
                buffer[2] = inp[0];
                if has_alpha {
                    buffer[3] = if has_key && key == Some((buffer[0] as u16, buffer[1] as u16, buffer[2] as u16)) {
                        0
                    } else {
                        255
                    };
                };
            }
        },
        ColorType::BGRA => {
            for (buffer, inp) in buffer.chunks_exact_mut(num_channels).zip(inp.chunks_exact(4)) {
                buffer[0] = inp[2];
                buffer[1] = inp[1];
                buffer[2] = inp[0];
                if has_alpha {
                    buffer[3] = inp[3];
                }
            }
        },
    };
}
/*Get RGBA16 color of pixel with index i (y * width + x) from the raw image with
given color type, but the given color type must be 16-bit itself.*/
#[inline(always)]
fn get_pixel_color_rgba16(inp: &[u8], mode: &ColorMode) -> RGBA16 {
    match mode.colortype {
        ColorType::GREY => {
            if inp.len() < 2 {
                return RGBA16::new(0,0,0,0);
            }
            let t = 256 * inp[0] as u16 + inp[1] as u16;
            RGBA16::new(t,t,t,
            if mode.key() == Some((t,t,t)) {
                0
            } else {
                0xffff
            })
        },
        ColorType::RGB => {
            if inp.len() < 6 {
                return RGBA16::new(0,0,0,0);
            }

            let r = 256 * inp[0] as u16 + inp[1] as u16;
            let g = 256 * inp[2] as u16 + inp[3] as u16;
            let b = 256 * inp[4] as u16 + inp[5] as u16;
            let a = if mode.key() == Some((r, g, b)) {
                0
            } else {
                0xffff
            };
            RGBA16::new(r, g, b, a)
        },
        ColorType::GREY_ALPHA => {
            if inp.len() < 4 {
                return RGBA16::new(0,0,0,0);
            }

            let t = 256 * inp[0] as u16 + inp[1] as u16;
            let a = 256 * inp[2] as u16 + inp[3] as u16;
            RGBA16::new(t, t, t, a)
        },
        ColorType::RGBA => {
            if inp.len() < 8 {
                return RGBA16::new(0,0,0,0);
            }

            RGBA16::new(
                256 * inp[0] as u16 + inp[1] as u16,
                256 * inp[2] as u16 + inp[3] as u16,
                256 * inp[4] as u16 + inp[5] as u16,
                256 * inp[6] as u16 + inp[7] as u16,
            )
        },
        ColorType::BGR |
        ColorType::BGRA |
        ColorType::BGRX |
        ColorType::PALETTE => RGBA16::new(0,0,0,0),
    }
}

#[inline(always)]
fn read_bits_from_reversed_stream(bitpointer: usize, bitstream: &[u8], nbits: usize) -> u32 {
    let mut result = 0;
    for bitpointer in bitpointer..bitpointer+nbits {
        result <<= 1;
        result |= read_bit_from_reversed_stream(bitpointer, bitstream) as u32;
    }
    result
}

fn read_chunk_plte(color: &mut ColorMode, data: &[u8]) -> Result<(), Error> {
    color.palette_clear();
    for c in data.chunks_exact(3).take(data.len() / 3) {
        color.palette_add(RGBA {
            r: c[0],
            g: c[1],
            b: c[2],
            a: 255,
        })?;
    }
    Ok(())
}

fn read_chunk_trns(color: &mut ColorMode, data: &[u8]) -> Result<(), Error> {
    if color.colortype == ColorType::PALETTE {
        let pal = color.palette_mut();
        if data.len() > pal.len() {
            return Err(Error::new(38));
        }
        for (i, &d) in data.iter().enumerate() {
            pal[i].a = d;
        }
    } else if color.colortype == ColorType::GREY {
        if data.len() != 2 {
            return Err(Error::new(30));
        }
        let t = 256 * data[0] as u16 + data[1] as u16;
        color.set_key(t, t, t);
    } else if color.colortype == ColorType::RGB {
        if data.len() != 6 {
            return Err(Error::new(41));
        }
        color.set_key(
            256 * data[0] as u16 + data[1] as u16,
            256 * data[2] as u16 + data[3] as u16,
            256 * data[4] as u16 + data[5] as u16,
        );
    } else {
        return Err(Error::new(42));
    }
    Ok(())
}

/*background color chunk (bKGD)*/
fn read_chunk_bkgd(info: &mut Info, data: &[u8]) -> Result<(), Error> {
    let chunk_length = data.len();
    if info.color.colortype == ColorType::PALETTE {
        /*error: this chunk must be 1 byte for indexed color image*/
        if chunk_length != 1 {
            return Err(Error::new(43)); /*error: this chunk must be 2 bytes for greyscale image*/
        } /*error: this chunk must be 6 bytes for greyscale image*/
        info.background_defined = true; /* OK */
        info.background_r = {
            info.background_g = {
                info.background_b = data[0].into();
                info.background_b
            };
            info.background_g
        };
    } else if info.color.colortype == ColorType::GREY || info.color.colortype == ColorType::GREY_ALPHA {
        if chunk_length != 2 {
            return Err(Error::new(44));
        }
        info.background_defined = true;
        info.background_r = {
            info.background_g = {
                info.background_b = 256 * data[0] as u16 + data[1] as u16;
                info.background_b
            };
            info.background_g
        };
    } else if info.color.colortype == ColorType::RGB || info.color.colortype == ColorType::RGBA {
        if chunk_length != 6 {
            return Err(Error::new(45));
        }
        info.background_defined = true;
        info.background_r = 256 * data[0] as u16 + data[1] as u16;
        info.background_g = 256 * data[2] as u16 + data[3] as u16;
        info.background_b = 256 * data[4] as u16 + data[5] as u16;
    }
    Ok(())
}
/*text chunk (tEXt)*/
fn read_chunk_text(info: &mut Info, data: &[u8]) -> Result<(), Error> {
    let (keyword, str) = split_at_nul(data);
    if keyword.is_empty() || keyword.len() > 79 {
        return Err(Error::new(89));
    }
    /*even though it's not allowed by the standard, no error is thrown if
        there's no null termination char, if the text is empty*/
    info.push_text(keyword, str)
}

/*compressed text chunk (zTXt)*/
fn read_chunk_ztxt(info: &mut Info, zlibsettings: &DecompressSettings, data: &[u8]) -> Result<(), Error> {
    let mut length = 0;
    while length < data.len() && data[length] != 0 {
        length += 1;
    }
    if length + 2 >= data.len() {
        return Err(Error::new(75));
    }
    if length < 1 || length > 79 {
        return Err(Error::new(89));
    }
    let key = &data[0..length];
    if data[length + 1] != 0 {
        return Err(Error::new(72));
    }
    /*the 0 byte indicating compression must be 0*/
    let string2_begin = length + 2; /*no null termination, corrupt?*/
    if string2_begin > data.len() {
        return Err(Error::new(75)); /*will fail if zlib error, e.g. if length is too small*/
    }
    let inl = &data[string2_begin..];
    let decoded = zlib::decompress(inl, zlibsettings)?;
    info.push_text(key, &decoded)?;
    Ok(())
}

fn split_at_nul(data: &[u8]) -> (&[u8], &[u8]) {
    let mut part = data.splitn(2, |&b| b == 0);
    (part.next().unwrap(), part.next().unwrap_or(&data[0..0]))
}

/*international text chunk (iTXt)*/
fn read_chunk_itxt(info: &mut Info, zlibsettings: &DecompressSettings, data: &[u8]) -> Result<(), Error> {
    /*Quick check if the chunk length isn't too small. Even without check
        it'd still fail with other error checks below if it's too short. This just gives a different error code.*/
    if data.len() < 5 {
        /*iTXt chunk too short*/
        return Err(Error::new(30));
    }

    let (key, data) = split_at_nul(data);
    if key.is_empty() || key.len() > 79 {
        return Err(Error::new(89));
    }
    if data.len() < 2 {
        return Err(Error::new(75));
    }
    let compressed_flag = data[0] != 0;
    if data[1] != 0 {
        return Err(Error::new(72));
    }
    let (langtag, data) = split_at_nul(&data[2..]);
    let (transkey, data) = split_at_nul(data);

    let decoded;
    let rest = if compressed_flag {
        decoded = zlib::decompress(data, zlibsettings)?;
        &decoded[..]
    } else {
        data
    };
    info.push_itext(key, langtag, transkey, rest)?;
    Ok(())
}

fn read_chunk_time(info: &mut Info, data: &[u8]) -> Result<(), Error> {
    let chunk_length = data.len();
    if chunk_length != 7 {
        return Err(Error::new(73));
    }
    info.time_defined = true;
    info.time.year = 256 * data[0] as u16 + data[1] as u16;
    info.time.month = data[2];
    info.time.day = data[3];
    info.time.hour = data[4];
    info.time.minute = data[5];
    info.time.second = data[6];
    Ok(())
}

fn read_chunk_phys(info: &mut Info, data: &[u8]) -> Result<(), Error> {
    let chunk_length = data.len();
    if chunk_length != 9 {
        return Err(Error::new(74));
    }
    info.phys_defined = true;
    info.phys_x = 16777216 * data[0] as u32 + 65536 * data[1] as u32 + 256 * data[2] as u32 + data[3] as u32;
    info.phys_y = 16777216 * data[4] as u32 + 65536 * data[5] as u32 + 256 * data[6] as u32 + data[7] as u32;
    info.phys_unit = data[8];
    Ok(())
}

#[inline(never)]
fn add_chunk_idat(out: &mut Vec<u8>, inp: &[u8], w: u32, h: u32, info_png: &Info, settings: &EncoderSettings, zlibsettings: &CompressSettings) -> Result<(), Error> {
    let mut ch = ChunkBuilder::new(out, b"IDAT");

    #[allow(deprecated)]
    if let Some(cb) = zlibsettings.custom_zlib {
        let mut tmp = Vec::new();
        filtered_scanlines(&mut tmp, inp, w, h, info_png, settings)?;
        (cb)(&tmp, &mut ch, zlibsettings)?;
    } else {
        let mut z = zlib::new_compressor(ch, zlibsettings);
        filtered_scanlines(&mut z, inp, w, h, info_png, settings)?;
        ch = z.finish()?;
    }
    ch.finish()
}

fn add_chunk_iend(out: &mut Vec<u8>) -> Result<(), Error> {
    ChunkBuilder::new(out, b"IEND").finish()
}

fn add_chunk_text(out: &mut Vec<u8>, keyword: &[u8], textstring: &[u8]) -> Result<(), Error> {
    if keyword.is_empty() || keyword.len() > 79 {
        return Err(Error::new(89));
    }
    let mut text = ChunkBuilder::new(out, b"tEXt");
    text.extend_from_slice(keyword)?;
    text.push(0);
    text.extend_from_slice(textstring)?;
    text.finish()
}

fn add_chunk_ztxt(out: &mut Vec<u8>, keyword: &[u8], textstring: &[u8], zlibsettings: &CompressSettings) -> Result<(), Error> {
    if keyword.is_empty() || keyword.len() > 79 {
        return Err(Error::new(89));
    }
    let mut data = ChunkBuilder::new(out, b"zTXt");
    data.extend_from_slice(keyword)?;
    data.push(0);
    data.push(0);
    zlib::compress_into(&mut data, textstring, zlibsettings)?;
    data.finish()
}

fn add_chunk_itxt(
    out: &mut Vec<u8>, compressed: bool, keyword: &str, langtag: &str, transkey: &str, textstring: &str, zlibsettings: &CompressSettings,
) -> Result<(), Error> {
    let k_len = keyword.len();
    if k_len < 1 || k_len > 79 {
        return Err(Error::new(89));
    }
    let mut data = ChunkBuilder::new(out, b"iTXt");
    data.extend_from_slice(keyword.as_bytes())?; data.push(0);
    data.push(compressed as u8);
    data.push(0);
    data.extend_from_slice(langtag.as_bytes())?; data.push(0);
    data.extend_from_slice(transkey.as_bytes())?; data.push(0);
    if compressed {
        zlib::compress_into(&mut data, textstring.as_bytes(), zlibsettings)?;
    } else {
        data.extend_from_slice(textstring.as_bytes())?;
    }
    data.finish()
}

fn add_chunk_bkgd(out: &mut Vec<u8>, info: &Info) -> Result<(), Error> {
    let mut bkgd = ChunkBuilder::new(out, b"bKGD");
    if info.color.colortype == ColorType::GREY || info.color.colortype == ColorType::GREY_ALPHA {
        bkgd.write_u16be(info.background_r);
    } else if info.color.colortype == ColorType::RGB || info.color.colortype == ColorType::RGBA {
        bkgd.write_u16be(info.background_r);
        bkgd.write_u16be(info.background_g);
        bkgd.write_u16be(info.background_b);
    } else if info.color.colortype == ColorType::PALETTE {
        bkgd.push((info.background_r & 255) as u8);
    }
    bkgd.finish()
}

fn add_chunk_ihdr(out: &mut Vec<u8>, w: u32, h: u32, colortype: ColorType, bitdepth: u8, interlace_method: u8) -> Result<(), Error> {
    let mut header = ChunkBuilder::new(out, b"IHDR");
    header.write_u32be(w);
    header.write_u32be(h);
    header.push(bitdepth);
    header.push(colortype as u8);
    header.push(0);
    header.push(0);
    header.push(interlace_method);
    header.finish()
}

fn add_chunk_trns(out: &mut Vec<u8>, info: &ColorMode) -> Result<(), Error> {
    let mut trns = ChunkBuilder::new(out, b"tRNS");
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
            trns.push(p.a);
        }
    } else if info.colortype == ColorType::GREY {
        if let Some((r, _, _)) = info.key() {
            trns.write_u16be(r);
        };
    } else if info.colortype == ColorType::RGB {
        if let Some((r, g, b)) = info.key() {
            trns.write_u16be(r);
            trns.write_u16be(g);
            trns.write_u16be(b);
        };
    }
    trns.finish()
}

fn add_chunk_plte(out: &mut Vec<u8>, info: &ColorMode) -> Result<(), Error> {
    let mut plte = ChunkBuilder::new(out, b"PLTE");
    for p in info.palette() {
        plte.push(p.r);
        plte.push(p.g);
        plte.push(p.b);
    }
    plte.finish()
}

fn add_chunk_time(out: &mut Vec<u8>, time: &Time) -> Result<(), Error> {
    let mut c = ChunkBuilder::new(out, b"tIME");
    c.write_u16be(time.year);
    c.extend_from_slice(&[
        time.month,
        time.day,
        time.hour,
        time.minute,
        time.second,
    ])?;
    c.finish()
}

fn add_chunk_phys(out: &mut Vec<u8>, info: &Info) -> Result<(), Error> {
    let mut data = ChunkBuilder::new(out, b"pHYs");
    data.write_u32be(info.phys_x);
    data.write_u32be(info.phys_y);
    data.push(info.phys_unit);
    data.finish()
}

pub(crate) struct ChunkBuilder<'buf> {
    buf: &'buf mut Vec<u8>,
    buf_start: usize,
    crc_hasher: crc32fast::Hasher,
}

impl<'buf> ChunkBuilder<'buf> {
    #[inline]
    #[must_use]
    pub fn new(buf: &'buf mut Vec<u8>, type_: &[u8; 4]) -> Self {
        let mut new = Self {
            buf_start: buf.len(),
            crc_hasher: crc32fast::Hasher::new(),
            buf,
        };
        new.buf.extend_from_slice(&[0,0,0,0]); // this will be length; excluded from crc
        let _ = new.extend_from_slice(&type_[..]); // included in crc
        debug_assert_eq!(8, new.buf.len() - new.buf_start);
        new
    }

    #[inline]
    pub fn write_u32be(&mut self, num: u32) {
        let _ = self.extend_from_slice(&num.to_be_bytes());
    }

    #[inline]
    pub fn write_u16be(&mut self, num: u16) {
        let _ = self.extend_from_slice(&num.to_be_bytes());
    }

    #[inline]
    pub fn push(&mut self, byte: u8) {
        self.buf.push(byte);
        self.crc_hasher.update(std::slice::from_ref(&byte));
    }

    #[inline]
    pub fn extend_from_slice(&mut self, slice: &[u8]) -> Result<(), Error> {
        self.buf.try_reserve(slice.len())?;
        self.buf.extend_from_slice(slice);
        self.crc_hasher.update(slice);
        Ok(())
    }

    pub fn finish(self) -> Result<(), Error> {
        let crc = self.crc_hasher.finalize();

        let written = self.buf.len() - self.buf_start;
        debug_assert!(written >= 8);
        let data_length = written - 8;
        debug_assert!(data_length < (1 << 31));
        if data_length > (1 << 31) {
            return Err(Error::new(77));
        }
        let len_range = self.buf_start .. self.buf_start + 4;
        self.buf[len_range].copy_from_slice(&(data_length as u32).to_be_bytes());

        self.buf.extend_from_slice(&crc.to_be_bytes());
        debug_assert!(ChunkRef::new(&self.buf[self.buf_start..]).unwrap().check_crc());
        Ok(())
    }
}

impl Write for ChunkBuilder<'_> {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_all(buf)?;
        Ok(buf.len())
    }
    #[inline(always)]
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.extend_from_slice(buf).map_err(|_| io::ErrorKind::OutOfMemory)?;
        Ok(())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

const ADAM7: [AdamConst; 7] = [
    AdamConst {ix: 0, iy: 0, dx: 8, dy: 8, },
    AdamConst {ix: 4, iy: 0, dx: 8, dy: 8, },
    AdamConst {ix: 0, iy: 4, dx: 4, dy: 8, },
    AdamConst {ix: 2, iy: 0, dx: 4, dy: 4, },
    AdamConst {ix: 0, iy: 2, dx: 2, dy: 4, },
    AdamConst {ix: 1, iy: 0, dx: 2, dy: 2, },
    AdamConst {ix: 0, iy: 1, dx: 1, dy: 2, },
];

/// shared values used by multiple Adam7 related functions
#[derive(Copy, Clone)]
struct AdamConst {
    /// x start values
    ix: u8,
    /// y start values
    iy: u8,
    /// x delta values
    dx: u8,
    dy: u8,
}

struct AdamPass {
    filtered_len: usize,
    padded_len: usize,
    packed_len: usize,
    w: u32,
    h: u32,
    adam: AdamConst,
}

#[inline]
fn adam7_pass_values(w: u32, h: u32, bpp: NonZeroU8) -> impl Iterator<Item=AdamPass> + Clone {
    ADAM7.iter().map(move |adam| {
        let mut p_w = ((w as usize + adam.dx as usize - adam.ix as usize - 1) / adam.dx as usize) as u32;
        let mut p_h = ((h as usize + adam.dy as usize - adam.iy as usize - 1) / adam.dy as usize) as u32;
        let filtered_len = if p_w != 0 && p_h != 0 {
            p_h as usize * (1 + linebytes_rounded(p_w, bpp))
        } else {
            if p_h == 0 {
                p_w = 0;
            }
            if p_w == 0 {
                p_h = 0;
            }
            0
        };
        AdamPass {
            adam: *adam,
            w: p_w,
            h: p_h,
            filtered_len,
            padded_len: p_h as usize * linebytes_rounded(p_w, bpp),
            packed_len: (p_h as usize * p_w as usize * bpp.get() as usize + 7) / 8,
        }
    })
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
#[cold]
fn adam7_deinterlace(out: &mut [u8], inp: &[u8], w: u32, h: u32, bpp: NonZeroU8) {
    let passes = adam7_pass_values(w, h, bpp);
    let bpp = bpp.get();
    let bytewidth = bpp / 8;
    let mut offset_packed = 0;
    if bpp >= 8 {
        for pass in passes {
            let adam = &pass.adam;
            for y in 0..pass.h as usize {
                for x in 0..pass.w as usize {
                    let pixelinstart = offset_packed + (y * pass.w as usize + x) * bytewidth as usize;
                    let pixeloutstart = ((adam.iy as usize + y * adam.dy as usize) * w as usize + adam.ix as usize + x * adam.dx as usize) * bytewidth as usize;
                    out[pixeloutstart..(bytewidth as usize + pixeloutstart)].copy_from_slice(&inp[pixelinstart..(bytewidth as usize + pixelinstart)]);
                }
            }
            offset_packed += pass.packed_len;
        }
    } else {
        for pass in passes {
            let adam = &pass.adam;
            let ilinebits = bpp as usize * pass.w as usize;
            let olinebits = bpp as usize * w as usize;
            for y in 0..pass.h as usize {
                for x in 0..pass.w as usize {
                    let mut ibp = (8 * offset_packed) + (y * ilinebits + x * bpp as usize);
                    let mut obp = (adam.iy as usize + y * adam.dy as usize) *
                        olinebits + (adam.ix as usize + x * adam.dx as usize) * bpp as usize;
                    for _ in 0..bpp {
                        let bit = read_bit_from_reversed_stream(ibp, inp); ibp += 1;
                        /*note that this function assumes the out buffer is completely 0, use set_bit_of_reversed_stream otherwise*/
                        set_bit_of_reversed_stream0(obp, out, bit); obp += 1;
                    }
                }
            }
            offset_packed += pass.packed_len;
        }
    }
}

/* ////////////////////////////////////////////////////////////////////////// */
/* / Reading and writing single bits and bytes from/to stream for LodePNG   / */
/* ////////////////////////////////////////////////////////////////////////// */
#[inline(always)]
fn read_bit_from_reversed_stream(bitpointer: usize, bitstream: &[u8]) -> u8 {
    (bitstream[bitpointer >> 3] >> (7 - (bitpointer & 7))) & 1
}

fn set_bit_of_reversed_stream0(bitpointer: usize, bitstream: &mut [u8], bit: u8) {
    /*the current bit in bitstream must be 0 for this to work*/
    if bit != 0 {
        /*earlier bit of huffman code is in a lesser significant bit of an earlier byte*/
        bitstream[bitpointer >> 3] |= bit << (7 - (bitpointer & 7));
    }
}

fn set_bit_of_reversed_stream(bitpointer: usize, bitstream: &mut [u8], bit: u8) {
    /*the current bit in bitstream may be 0 or 1 for this to work*/
    if bit == 0 {
        bitstream[bitpointer >> 3] &= (!(1 << (7 - (bitpointer & 7)))) as u8;
    } else {
        bitstream[bitpointer >> 3] |= 1 << (7 - (bitpointer & 7));
    }
}
/* ////////////////////////////////////////////////////////////////////////// */
/* / PNG chunks                                                             / */
/* ////////////////////////////////////////////////////////////////////////// */
#[inline]
pub(crate) fn chunk_length(chunk: &[u8]) -> usize {
    u32::from_be_bytes(chunk[..4].try_into().unwrap()) as usize
}

pub(crate) fn lodepng_chunk_generate_crc(chunk: &mut [u8]) {
    let ch = ChunkRef::new(chunk).unwrap();
    let length = ch.len();
    let crc = ch.crc();
    chunk[8 + length..].copy_from_slice(&crc.to_be_bytes());
}

/* ////////////////////////////////////////////////////////////////////////// */
/* / Color types and such                                                   / */
/* ////////////////////////////////////////////////////////////////////////// */
fn check_png_color_validity(colortype: ColorType, bd: u32) -> Result<(), Error> {
    /*allowed color type / bits combination*/
    match colortype {
        ColorType::GREY => if !(bd == 1 || bd == 2 || bd == 4 || bd == 8 || bd == 16) {
            return Err(Error::new(37));
        },
        ColorType::PALETTE => if !(bd == 1 || bd == 2 || bd == 4 || bd == 8) {
            return Err(Error::new(37));
        },
        ColorType::RGB | ColorType::GREY_ALPHA | ColorType::RGBA => if !(bd == 8 || bd == 16) {
            return Err(Error::new(37));
        },
        _ => {
            return Err(Error::new(31))
        },
    }
    Ok(())
}
/// Internally BGRA is allowed
fn check_lode_color_validity(colortype: ColorType, bd: u32) -> Result<(), Error> {
    match colortype {
        ColorType::BGRA | ColorType::BGRX | ColorType::BGR if bd == 8 => {
            Ok(())
        },
        ct => check_png_color_validity(ct, bd),
    }
}

pub(crate) fn lodepng_color_mode_equal(a: &ColorMode, b: &ColorMode) -> bool {
    a.colortype == b.colortype &&
    a.bitdepth() == b.bitdepth() &&
    a.key() == b.key() &&
    a.palette() == b.palette()
}

/* ////////////////////////////////////////////////////////////////////////// */
/* / CRC32                                                                  / */
/* ////////////////////////////////////////////////////////////////////////// */

#[test]
fn check_crc_impl() {

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

    fn lodepng_crc32_old(data: &[u8]) -> u32 {
        let mut r = 4294967295u32;
        for &d in data {
            r = LODEPNG_CRC32_TABLE[((r ^ d as u32) & 255) as usize] ^ (r >> 8);
        }
        r ^ 4294967295
    }
    for data in [&b"hello world"[..], b"aaaaaaaaaaaaaaaaaaa", b"", b"123456123456123456123456123456123456123456\0\0\0\0\0\0\0\0\0"] {
        assert_eq!(lodepng_crc32_old(data), crc32fast::hash(data));
    }
}

#[inline(never)]
pub(crate) fn lodepng_convert(out: &mut [u8], inp: &[u8], mode_out: &ColorMode, mode_in: &ColorMode, w: u32, h: u32) -> Result<(), Error> {
    let numpixels = w as usize * h as usize;
    let bytewidth_in = (mode_in.bpp_().get() / 8) as usize;
    let bytewidth_out = (mode_out.bpp_().get() / 8) as usize;
    if mode_in.bitdepth > 16 || mode_out.bitdepth > 16 || mode_in.bitdepth == 0 || mode_out.bitdepth == 0 {
        return Err(Error::new(37));
    }
    if lodepng_color_mode_equal(mode_out, mode_in) {
        let numbytes = mode_in.raw_size_opt(w as _, h as _)?;
        out[..numbytes].copy_from_slice(&inp[..numbytes]);
        return Ok(());
    }
    let mut colormap = ColorIndices::new();
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
        colormap.extend(palette.iter().enumerate().map(|(i, p)| {
            (*p, i as u8)
        }));
    }
    if mode_in.bitdepth() == 16 && mode_out.bitdepth() == 16 && bytewidth_out > 0 && bytewidth_in > 0 {
        for (px, px_out) in inp.chunks_exact(bytewidth_in).zip(out.chunks_exact_mut(bytewidth_out)).take(numpixels) {
            let px = get_pixel_color_rgba16(px, mode_in);
            rgba16_to_pixel(px_out, mode_out, px);
        }
    } else if mode_out.bitdepth() == 8 && mode_out.colortype == ColorType::RGBA {
        get_pixel_colors_rgba8(&mut out[..numpixels * 4], true, inp, mode_in);
    } else if mode_out.bitdepth() == 8 && mode_out.colortype == ColorType::RGB {
        get_pixel_colors_rgba8(&mut out[..numpixels * 3], false, inp, mode_in);
    } else if mode_in.colortype == ColorType::PALETTE || mode_in.bpp_().get() < 8 {
        let mut gray_pal = [RGBA::new(0,0,0,0); 256];
        let pal = if mode_in.colortype == ColorType::PALETTE {
            mode_in.palette()
        } else {
            gray_palette(mode_in, &mut gray_pal)
        };
        for i in 0..numpixels {
            let index = get_pixel_low_bpp(inp, i, mode_in) as usize;
            /*This is an error according to the PNG spec, but common PNG decoders make it black instead.
              Done here too, slightly faster due to no error handling needed.*/
            let px = pal.get(index).copied().unwrap_or(RGBA::new(0, 0, 0, 255));
            rgba8_to_pixel(out, i, mode_out, &mut colormap, px)?;
        }
    } else if bytewidth_in > 0 {
        for (i, pixel_in) in inp.chunks_exact(bytewidth_in).enumerate().take(numpixels) {
            let px = get_pixel_color_rgba8(pixel_in, mode_in);
            rgba8_to_pixel(out, i, mode_out, &mut colormap, px)?;
        }
    }
    Ok(())
}

fn gray_palette<'a>(mode: &ColorMode, gray_pal: &'a mut [RGBA; 256]) -> &'a [RGBA] {
    let colors = 1 << mode.bitdepth();
    gray_pal.iter_mut().enumerate().take(colors).for_each(|(value, pal)| {
        let t = ((value * 255) / (colors - 1)) as u8;
        let a = if mode.key() == Some((t as u16, t as u16, t as u16)) {
            0
        } else {
            255
        };
        *pal = RGBA::new(t, t, t, a);
    });
    gray_pal
}


/*out must be buffer big enough to contain full image, and in must contain the full decompressed data from
the IDAT chunks (with filter index bytes and possible padding bits)
return value is error*/
/*
  This function converts the filtered-padded-interlaced data into pure 2D image buffer with the PNG's colortype.
  Steps:
  *) if no Adam7: 1) unfilter 2) remove padding bits (= posible extra bits per scanline if bpp < 8)
  *) if adam7: 1) 7x unfilter 2) 7x remove padding bits 3) adam7_deinterlace
  NOTE: the in buffer will be overwritten with intermediate data!
  */
fn postprocess_scanlines(mut inp: Vec<u8>, unfiltering_buffer: usize, w: u32, h: u32, info_png: &Info) -> Result<Vec<u8>, Error> {
    let bpp = info_png.color.bpp_();
    let raw_size = info_png.color.raw_size_opt(w, h)?;
    Ok(if info_png.interlace_method == 0 {
        if bpp.get() < 8 && linebits_exact(w, bpp) != linebits_rounded(w, bpp) {
            unfilter_scanlines(&mut inp, unfiltering_buffer, w, h, bpp)?;
            let mut out = zero_vec(raw_size)?;
            remove_padding_bits(&mut out, &inp, linebits_exact(w, bpp), linebits_rounded(w, bpp), h);
            out
        } else {
            unfilter_scanlines(&mut inp, unfiltering_buffer, w, h, bpp)?;
            inp.truncate(raw_size);
            inp
        }
    } else {
        let passes = adam7_pass_values(w, h, bpp);
        let mut offset_padded = 0;
        let mut offset_filtered = unfiltering_buffer;
        let mut offset_packed = 0;
        for pass in passes {
            unfilter_scanlines(&mut inp[offset_padded..], offset_filtered-offset_padded, pass.w, pass.h, bpp)?;
            if bpp.get() < 8 {
                /*remove padding bits in scanlines; after this there still may be padding
                        bits between the different reduced images: each reduced image still starts nicely at a byte*/
                remove_padding_bits_aliased(&mut inp, offset_packed, offset_padded, linebits_exact(pass.w as _, bpp), linebits_rounded(pass.w as _, bpp), pass.h);
            };
            offset_padded += pass.padded_len;
            offset_filtered += pass.filtered_len;
            offset_packed += pass.packed_len;
        }
        let mut out = zero_vec(raw_size)?;
        adam7_deinterlace(&mut out, &inp[unfiltering_buffer..], w, h, bpp);
        out
    })
}

#[inline(never)]
fn unfilter_scanlines(mut inout: &mut [u8], input_offset: usize, w: u32, h: u32, bpp: NonZeroU8) -> Result<(), Error> {
    let bytewidth = (bpp.get() + 7) / 8;
    let linebytes = linebytes_rounded(w, bpp);
    if linebytes == 0 {
        return Ok(())
    }

    let mut prev_line = None;
    for input_offset in input_offset..input_offset+h as usize {
        if inout.len() < linebytes { return Err(Error::new(77)); }
        let filter_type = *inout.get(input_offset).ok_or(Error::new(77))?;
        prev_line = if (filter_type == 1 || (filter_type == 3 && prev_line.is_some()) || (filter_type == 4 && prev_line.is_none())) && input_offset >= linebytes {
            let (out, rest) = inout.split_at_mut(linebytes);
            let inp = rest.get((input_offset + 1 - linebytes)..=input_offset).ok_or(Error::new(77))?;
            if filter_type == 1 || filter_type == 4 {
                unfilter_scanline_filter_1(out, inp, bytewidth)?;
            } else if let Some(prev) = prev_line {
                unfilter_scanline_filter_3(out, inp, prev, bytewidth)?;
            }
            inout = rest;
            Some(out)
        } else {
            unfilter_scanline_aliased(inout, input_offset + 1, prev_line, bytewidth, filter_type, linebytes)
                .ok_or_else(|| Error::new(77))?;
            let (out, rest) = inout.split_at_mut(linebytes);
            inout = rest;
            Some(out)
        };
    }
    Ok(())
}

#[inline(never)]
fn unfilter_scanline_filter_1(out: &mut [u8], scanline: &[u8], bytewidth: u8) -> Result<(), Error> {
    let bytewidth = bytewidth as usize;
    let length = out.len();
    debug_assert_eq!(scanline.len(), length);
    // help the optimizer remove bounds checks
    if bytewidth > 8 || bytewidth < 1 || length < bytewidth || length != scanline.len() || length % bytewidth != 0 {
        debug_assert!(false);
        return Err(Error::new(84));
    }

    match bytewidth {
        1 => filter_1::<1>(out, scanline),
        2 => filter_1::<2>(out, scanline),
        3 => filter_1::<3>(out, scanline),
        4 => filter_1::<4>(out, scanline),
        6 => filter_1::<6>(out, scanline),
        8 => filter_1::<8>(out, scanline),
        _ => {},
    }
    Ok(())
}

#[inline(never)]
fn unfilter_scanline_filter_3(out: &mut [u8], scanline: &[u8], prevline: &[u8], bytewidth: u8) -> Result<(), Error> {
    let bytewidth = bytewidth as usize;
    let length = out.len();
    debug_assert_eq!(scanline.len(), length);
    // help the optimizer remove bounds checks
    if bytewidth > 8 || bytewidth < 1 || length < bytewidth || length != scanline.len() || prevline.len() != length || length % bytewidth != 0 {
        debug_assert!(false);
        return Err(Error::new(84));
    }
    match bytewidth {
        1 => filter_3::<1>(out, scanline, prevline),
        2 => filter_3::<2>(out, scanline, prevline),
        3 => filter_3::<3>(out, scanline, prevline),
        4 => filter_3::<4>(out, scanline, prevline),
        6 => filter_3::<6>(out, scanline, prevline),
        8 => filter_3::<8>(out, scanline, prevline),
        _ => {},
    }
    Ok(())
}

#[inline(always)]
fn filter_3<const N: usize>(out: &mut [u8], scanline: &[u8], prevline: &[u8]) {
    let mut out_prev = [0; N];
    let out = out.chunks_exact_mut(N);
    let scanline = scanline.chunks_exact(N);
    let prevline = prevline.chunks_exact(N);

    for (out, (s, up)) in out.zip(scanline.zip(prevline)) {
        for (out, (s, (up, prev))) in out.iter_mut().zip(s.iter().copied().zip(up.iter().copied().zip(out_prev.iter().copied()))) {
            let t = up as u16 + prev as u16;
            *out = s.wrapping_add((t >> 1) as u8);
        }
        out_prev.copy_from_slice(out);
    }
}

#[inline(always)]
fn filter_1<const N: usize>(out: &mut [u8], scanline: &[u8]) {
    let out = out.chunks_exact_mut(N);
    let scanline = scanline.chunks_exact(N);
    let mut out_prev = [0; N];
    out.zip(scanline).for_each(|(out, s)| {
        out.iter_mut().zip(s.iter().copied().zip(out_prev.iter().copied())).for_each(|(out, (s, out_prev))| {
            *out = s.wrapping_add(out_prev);
        });
        out_prev.copy_from_slice(out);
    });
}

/// IDAT decompression ensures this is never needed
fn unfilter_scanline_aliased(inout: &mut [u8], scanline_offset: usize, prevline: Option<&[u8]>, bytewidth: u8, filter_type: u8, length: usize) -> Option<()> {
    let bytewidth = bytewidth as usize;
    // help the optimizer remove bounds checks
    if bytewidth > 8 || bytewidth < 1 || bytewidth > length || prevline.map_or(false, move |p| p.len() != length) || inout.len() < scanline_offset+bytewidth {
        debug_assert!(false);
        return None;
    }

    match (filter_type, prevline) {
        (0, _) | (2, None) => {
            inout.copy_within(scanline_offset..scanline_offset.checked_add(length)?, 0);
        },
        (1, _) | (4, None) => {
            inout.copy_within(scanline_offset..scanline_offset.checked_add(bytewidth)?, 0);

            let inout = Cell::from_mut(inout).as_slice_of_cells();
            let out = inout.get(..length)?;
            let scanline = inout.get(scanline_offset..scanline_offset+length)?;

            let scanline_next = &scanline[bytewidth..];
            let out_prev = &out[..length-bytewidth];
            let out_next = &out[bytewidth..];
            for (out, (s, out_prev)) in out_next.iter().zip(scanline_next.iter().zip(out_prev)) {
                out.set(s.get().wrapping_add(out_prev.get()));
            }
        },
        (2, Some(prevline)) => {
            let prevline = prevline.get(..length)?;

            let inout = Cell::from_mut(inout).as_slice_of_cells();
            let out = inout.get(..length)?;
            let scanline = inout.get(scanline_offset..scanline_offset+length)?;

            for (out, (s, p)) in out.iter().zip(scanline.iter().zip(prevline.iter().copied())) {
                out.set(s.get().wrapping_add(p));
            }
        },
        (3, Some(prevline)) => {
                let prevline = prevline.get(..length)?;
                let (prevline_start, prevline_next) = prevline.split_at(bytewidth);

                let inout = Cell::from_mut(inout).as_slice_of_cells();
                let out = inout.get(..length)?;
                let scanline = inout.get(scanline_offset..scanline_offset+length)?;

                let (scanline_start, scanline_next) = scanline.split_at(bytewidth);
                let out_prev = &out[..length-bytewidth];
                let (out_start, out_next) = out.split_at(bytewidth);

                for (out, (s, p)) in out_start.iter().zip(scanline_start.iter().zip(prevline_start.iter().copied())) {
                    out.set(s.get().wrapping_add(p >> 1));
                }

                for ((out, out_prev), (s, p)) in out_next.iter().zip(out_prev).zip(scanline_next.iter().zip(prevline_next.iter().copied())) {
                    let t = out_prev.get() as u16 + p as u16;
                    out.set(s.get().wrapping_add((t >> 1) as u8));
                }
        },
        (3, None) => {
                inout.copy_within(scanline_offset..scanline_offset+bytewidth, 0);

                let inout = Cell::from_mut(inout).as_slice_of_cells();
                let out = inout.get(..length)?;
                let scanline = inout.get(scanline_offset..scanline_offset+length)?;

                let scanline_next = &scanline[bytewidth..];
                let out_prev = &out[..length-bytewidth];
                let out_next = &out[bytewidth..];
                for ((out, out_prev), s) in out_next.iter().zip(out_prev).zip(scanline_next.iter()) {
                    out.set(s.get().wrapping_add(out_prev.get() >> 1));
                }
        },
        (4, Some(prevline)) => {
                let prevline = prevline.get(..length)?;
                let (prevline_start, prevline_next) = prevline.split_at(bytewidth);
                let prevline_prev = &prevline[..length-bytewidth];

                let inout = Cell::from_mut(inout).as_slice_of_cells();
                let out = inout.get(..length)?;
                let scanline = inout.get(scanline_offset..scanline_offset+length)?;

                let (scanline_start, scanline_next) = scanline.split_at(bytewidth);
                let out_prev = &out[..length-bytewidth];
                let (out_start, out_next) = out.split_at(bytewidth);
                for (out, (s, p)) in out_start.iter().zip(scanline_start.iter().zip(prevline_start.iter().copied())) {
                    out.set(s.get().wrapping_add(p));
                }
                for ((out, out_prev), (s, (p, p_prev))) in out_next.iter().zip(out_prev).zip(scanline_next.iter().zip(prevline_next.iter().copied().zip(prevline_prev.iter().copied()))) {
                    let pred = paeth_predictor(out_prev.get(), p, p_prev);
                    out.set(s.get().wrapping_add(pred));
                }
        },
        (bad_filter, _) => {
            debug_assert!(bad_filter >= 5);
            return None
        },
    }
    Some(())
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
#[inline(never)]
fn remove_padding_bits(out: &mut [u8], inp: &[u8], olinebits: usize, ilinebits: usize, h: u32) {
    for y in 0..h as usize {
        let iline = y * ilinebits;
        let oline = y * olinebits;
        for i in 0..olinebits {
            let bit = read_bit_from_reversed_stream(iline + i, inp);
            set_bit_of_reversed_stream(oline + i, out, bit);
        }
    }
}

#[inline(never)]
fn remove_padding_bits_aliased(inout: &mut [u8], out_off: usize, in_off: usize, olinebits: usize, ilinebits: usize, h: u32) {
    for y in 0..h as usize {
        let iline = y * ilinebits;
        let oline = y * olinebits;
        for i in 0..olinebits {
            let bit = read_bit_from_reversed_stream(iline + i, &inout[in_off..]);
            set_bit_of_reversed_stream(oline + i, &mut inout[out_off..], bit);
        }
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
#[cold]
fn adam7_interlace(out: &mut [u8], inp: &[u8], w: u32, h: u32, bpp: NonZeroU8) {
    let passes = adam7_pass_values(w, h, bpp);
    let bpp = bpp.get() as usize;
    let mut offset_packed = 0;
    if bpp >= 8 {
        for pass in passes {
            let adam = &pass.adam;
            let bytewidth = bpp / 8;
            for y in 0..pass.h as usize {
                for x in 0..pass.w as usize {
                    let pixelinstart = ((adam.iy as usize + y * adam.dy as usize) * w as usize + adam.ix as usize + x * adam.dx as usize) * bytewidth;
                    let pixeloutstart = offset_packed + (y * pass.w as usize + x) * bytewidth;
                    out[pixeloutstart..(bytewidth + pixeloutstart)]
                        .copy_from_slice(&inp[pixelinstart..(bytewidth + pixelinstart)]);
                }
            }
            offset_packed += pass.packed_len;
        }
    } else {
        for pass in passes {
            let adam = &pass.adam;
            let ilinebits = bpp * pass.w as usize;
            let olinebits = bpp * w as usize;
            for y in 0..pass.h as usize {
                for x in 0..pass.w as usize {
                    let mut ibp = (adam.iy as usize + y * adam.dy as usize) * olinebits + (adam.ix as usize + x * adam.dx as usize) * bpp;
                    let mut obp = (8 * offset_packed) + (y * ilinebits + x * bpp);
                    for _ in 0..bpp {
                        let bit = read_bit_from_reversed_stream(ibp, inp); ibp += 1;
                        set_bit_of_reversed_stream(obp, out, bit); obp += 1;
                    }
                }
            }
            offset_packed += pass.packed_len;
        }
    }
}

/* ////////////////////////////////////////////////////////////////////////// */
/* / PNG Decoder                                                            / */
/* ////////////////////////////////////////////////////////////////////////// */
/*read the information from the header and store it in the Info. return value is error*/
#[inline(never)]
pub(crate) fn lodepng_inspect(decoder: &DecoderSettings, inp: &[u8], read_chunks: bool) -> Result<(Info, u32, u32), Error> {
    if inp.len() < 33 {
        /*error: the data length is smaller than the length of a PNG header*/
        return Err(Error::new(27));
    }
    /*when decoding a new PNG image, make sure all parameters created after previous decoding are reset*/
    if inp[0..8] != [137, 80, 78, 71, 13, 10, 26, 10] {
        /*error: the first 8 bytes are not the correct PNG signature*/
        return Err(Error::new(28));
    }
    let mut chunks = ChunksIter { data: &inp[8..] };
    let ihdr = chunks.next().ok_or(Error::new(28))??;
    if &ihdr.name() != b"IHDR" {
        /*error: it doesn't start with a IHDR chunk!*/
        return Err(Error::new(29));
    }
    if ihdr.len() != 13 {
        /*error: header size must be 13 bytes*/
        return Err(Error::new(94));
    }
    /*read the values given in the header*/
    let w = u32::from_be_bytes(inp[16..][..4].try_into().unwrap());
    let h = u32::from_be_bytes(inp[20..][..4].try_into().unwrap());
    if w == 0 || h == 0 {
        return Err(Error::new(93));
    }
    let bitdepth = inp[24];
    if bitdepth == 0 || bitdepth > 16 {
        return Err(Error::new(29));
    }
    let mut info_png = Info::new();
    info_png.color.try_set_bitdepth(inp[24] as u32)?;
    info_png.color.colortype = match inp[25] {
        0 => ColorType::GREY,
        2 => ColorType::RGB,
        3 => ColorType::PALETTE,
        4 => ColorType::GREY_ALPHA,
        6 => ColorType::RGBA,
        _ => return Err(Error::new(31)),
    };
    info_png.interlace_method = inp[28];
    if !decoder.ignore_crc && !ihdr.check_crc() {
        return Err(Error::new(57));
    }
    if info_png.interlace_method > 1 {
        /*error: only interlace methods 0 and 1 exist in the specification*/
        return Err(Error::new(34));
    }
    if read_chunks {
        for ch in chunks {
            let ch = ch?;
            match &ch.name() {
                b"IDAT" | b"IEND" => break,
                b"PLTE" => {
                    read_chunk_plte(&mut info_png.color, ch.data())?;
                },
                b"tRNS" => {
                    read_chunk_trns(&mut info_png.color, ch.data())?;
                },
                b"bKGD" => {
                    read_chunk_bkgd(&mut info_png, ch.data())?;
                },
                _ => {},
            }
        }
    }
    check_png_color_validity(info_png.color.colortype, info_png.color.bitdepth())?;
    Ok((info_png, w, h))
}

/*read a PNG, the result will be in the same color type as the PNG (hence "generic")*/
fn decode_generic(state: &mut State, inp: &[u8]) -> Result<(Vec<u8>, u32, u32), Error> {
    let mut found_iend = false; /*the data from idat chunks*/
    /*for unknown chunk order*/
    let mut unknown = false;
    let mut critical_pos = ChunkPosition::IHDR;
    /*provide some proper output values if error will happen*/
    let (info, w, h) = lodepng_inspect(&state.decoder, inp, false)?;
    state.info_png = info;

    /*reads header and resets other parameters in state->info_png*/
    let numpixels = match (w as usize).checked_mul(h as usize) {
        Some(n) => n,
        None => {
            return Err(Error::new(92));
        },
    };
    /*multiplication overflow possible further below. Allows up to 2^31-1 pixel
      bytes with 16-bit RGBA, the rest is room for filter bytes.*/
    if numpixels > (isize::MAX as usize - 1) / 4 / 2 {
        return Err(Error::new(92)); /*first byte of the first chunk after the header*/
    }

    let chunks = ChunksIter {
        data: inp.get(33..).ok_or(Error::new(27))?,
    };

    /*predict output size, to allocate exact size for output buffer to avoid more dynamic allocation.
      If the decompressed size does not match the prediction, the image must be corrupt.*/
    let predict = if state.info_png.interlace_method == 0 {
        /*The extra *h is added because this are the filter bytes every scanline starts with*/
        state.info_png.color.raw_size_idat(w, h).ok_or(Error::new(91))? + h as usize
    } else {
        /*Adam-7 interlaced: predicted size is the sum of the 7 sub-images sizes*/
        let color = &state.info_png.color;
        adam7_expected_size(color, w, h).ok_or(Error::new(91))?
    };

    let bpp = state.info_png.color.bpp_();
    let bytewidth = ((bpp.get() + 7) / 8) as usize;

    // if unfiltering can shift the buffer by two lines, it can do unaliased unfiltering in-place
    let unfiltering_buffer = if state.info_png.interlace_method == 0 { 2 * (1 + bytewidth * w as usize) } else { 0 };

    let mut scanlines = Vec::new();
    let capacity_required = predict + unfiltering_buffer;
    let remainder = capacity_required % bytewidth;
    // ensure the buffer is multiple of pixel size, so that it can be cheaply transmuted
    scanlines.try_reserve_exact(capacity_required + if remainder == 0 {0} else {bytewidth - remainder})?;

    scanlines.resize(unfiltering_buffer, 0);

    let mut idat_decompressor = zlib::new_decompressor(scanlines, chunks.data.len(), &state.decoder.zlibsettings);

    /*loop through the chunks, ignoring unknown chunks and stopping at IEND chunk.
      IDAT data is put at the start of the in buffer*/
    for ch in chunks {
        let ch = ch?;
        /*length of the data of the chunk, excluding the length bytes, chunk type and CRC bytes*/
        let data = ch.data();
        match &ch.name() {
            b"IDAT" => {
                idat_decompressor.push(data)?;
                critical_pos = ChunkPosition::IDAT;
            },
            b"IEND" => {
                found_iend = true;
            },
            b"PLTE" => {
                read_chunk_plte(&mut state.info_png.color, data)?;
                critical_pos = ChunkPosition::PLTE;
            },
            b"tRNS" => {
                read_chunk_trns(&mut state.info_png.color, data)?;
            },
            b"bKGD" => {
                read_chunk_bkgd(&mut state.info_png, data)?;
            },
            b"tEXt" => if state.decoder.read_text_chunks {
                read_chunk_text(&mut state.info_png, data)?;
            },
            b"zTXt" => if state.decoder.read_text_chunks {
                read_chunk_ztxt(&mut state.info_png, &state.decoder.zlibsettings, data)?;
            },
            b"iTXt" => if state.decoder.read_text_chunks {
                read_chunk_itxt(&mut state.info_png, &state.decoder.zlibsettings, data)?;
            },
            b"tIME" => {
                read_chunk_time(&mut state.info_png, data)?;
            },
            b"pHYs" => {
                read_chunk_phys(&mut state.info_png, data)?;
            },
            _ => {
                if !ch.is_ancillary() {
                    return Err(Error::new(69));
                }
                unknown = true;
                if state.decoder.remember_unknown_chunks {
                    state.info_png.push_unknown_chunk(critical_pos, ch.whole_chunk_data())?;
                }
            },
        };
        if !state.decoder.ignore_crc && !unknown && !ch.check_crc() {
            return Err(Error::new(57));
        }
        if found_iend {
            break;
        }
    }
    if !found_iend && !state.decoder.ignore_crc {
        return Err(Error::new(52));
    }
    let scanlines = idat_decompressor.finish()?;
    if scanlines.len() != predict + unfiltering_buffer {
        /*decompressed size doesn't match prediction*/
        return Err(Error::new(91));
    }
    let out = postprocess_scanlines(scanlines, unfiltering_buffer, w, h, &state.info_png)?;
    Ok((out, w, h))
}

fn adam7_expected_size(color: &ColorMode, w: u32, h: u32) -> Option<usize> {
    fn div_ceil(x: u32, d: u8) -> u32 {
        ((x as u64 + (d as u64 - 1)) / d as u64) as u32
    }

    fn div_round(x: u32, d: u8) -> u32 {
        ((x as u64 + ((d/2) as u64 - 1)) / d as u64) as u32
    }
    let mut predict = color.raw_size_idat(div_ceil(w, 8), div_ceil(h, 8))? + div_ceil(h, 8) as usize;
    if w > 4 {
        predict += color.raw_size_idat(div_round(w, 8), div_ceil(h, 8))? + div_ceil(h, 8) as usize;
    }
    predict += color.raw_size_idat(div_ceil(w, 4), div_round(h, 8))? + div_round(h, 8) as usize;
    if w > 2 {
        predict += color.raw_size_idat(div_round(w, 4), div_ceil(h, 4))? + div_ceil(h, 4) as usize;
    }
    predict += color.raw_size_idat(div_ceil(w, 2), div_round(h, 4))? + div_round(h, 4) as usize;
    if w > 1 {
        predict += color.raw_size_idat(w >> 1, div_ceil(h, 2))? + div_ceil(h, 2) as usize;
    }
    predict += color.raw_size_idat(w, h >> 1)? + (h >> 1) as usize;
    Some(predict)
}

#[inline(never)]
pub(crate) fn lodepng_decode(state: &mut State, inp: &[u8]) -> Result<(Vec<u8>, u32, u32), Error> {
    let (decoded, w, h) = decode_generic(state, inp)?;

    if !state.decoder.color_convert || lodepng_color_mode_equal(&state.info_raw, &state.info_png.color) {
        /*store the info_png color settings on the info_raw so that the info_raw still reflects what colortype
            the raw image has to the end user*/
        if !state.decoder.color_convert {
            /*color conversion needed; sort of copy of the data*/
            state.info_raw = state.info_png.color.clone();
        }
        Ok((decoded, w, h))
    } else {
        /*TODO: check if this works according to the statement in the documentation: "The converter can convert
            from greyscale input color type, to 8-bit greyscale or greyscale with alpha"*/
        if !(state.info_raw.colortype == ColorType::RGB || state.info_raw.colortype == ColorType::RGBA) && (state.info_raw.bitdepth() != 8) {
            return Err(Error::new(56)); /*unsupported color mode conversion*/
        }
        let mut out = zero_vec(state.info_raw.raw_size_opt(w, h)?)?;
        lodepng_convert(&mut out, &decoded, &state.info_raw, &state.info_png.color, w, h)?;
        Ok((out, w, h))
    }
}

#[inline]
pub(crate) fn lodepng_decode_memory(inp: &[u8], colortype: ColorType, bitdepth: u32) -> Result<(Vec<u8>, u32, u32), Error> {
    let mut state = Decoder::new();
    state.info_raw_mut().colortype = colortype;
    state.info_raw_mut().try_set_bitdepth(bitdepth)?;
    lodepng_decode(&mut state.state, inp)
}

#[inline]
fn add_unknown_chunks(out: &mut Vec<u8>, data: &[u8]) -> Result<(), Error> {
    debug_assert!(ChunksIter { data }.all(|ch| ch.is_ok()));
    out.try_reserve(data.len())?;
    out.extend_from_slice(data);
    Ok(())
}

pub const LODEPNG_VERSION_STRING: &[u8] = b"20161127-Rust-3.0\0";

#[inline(never)]
pub(crate) fn lodepng_encode(image: &[u8], w: u32, h: u32, state: &State) -> Result<Vec<u8>, Error> {
    if w == 0 || h == 0 {
        return Err(Error::new(93));
    }

    let mut info = state.info_png.clone();
    if (info.color.colortype == ColorType::PALETTE || state.encoder.force_palette) && (info.color.palette().is_empty() || info.color.palette().len() > 256) {
        return Err(Error::new(68));
    }
    if state.encoder.auto_convert {
        info.color = auto_choose_color(image, w, h, &state.info_raw)?;
    }
    if state.info_png.interlace_method > 1 {
        return Err(Error::new(71));
    }
    check_png_color_validity(info.color.colortype, info.color.bitdepth())?; /*tEXt and/or zTXt */
    check_lode_color_validity(state.info_raw.colortype, state.info_raw.bitdepth())?; /*LodePNG version id in text chunk */

    let mut outv = Vec::new(); outv.try_reserve(1024 + w as usize * h as usize / 2)?;
    write_signature(&mut outv);

    add_chunk_ihdr(&mut outv, w, h, info.color.colortype, info.color.bitdepth() as u8, info.interlace_method)?;
    add_unknown_chunks(&mut outv, &info.unknown_chunks[ChunkPosition::IHDR as usize])?;
    if info.color.colortype == ColorType::PALETTE {
        add_chunk_plte(&mut outv, &info.color)?;
    }
    if state.encoder.force_palette && (info.color.colortype == ColorType::RGB || info.color.colortype == ColorType::RGBA) {
        add_chunk_plte(&mut outv, &info.color)?;
    }
    if info.color.colortype == ColorType::PALETTE && get_palette_translucency(info.color.palette()) != PaletteTranslucency::Opaque {
        add_chunk_trns(&mut outv, &info.color)?;
    }
    if (info.color.colortype == ColorType::GREY || info.color.colortype == ColorType::RGB) && info.color.key().is_some() {
        add_chunk_trns(&mut outv, &info.color)?;
    }
    if info.background_defined {
        add_chunk_bkgd(&mut outv, &info)?;
    }
    if info.phys_defined {
        add_chunk_phys(&mut outv, &info)?;
    }
    add_unknown_chunks(&mut outv, &info.unknown_chunks[ChunkPosition::PLTE as usize])?;

    let mut converted;
    let mut image = image;
    if !lodepng_color_mode_equal(&state.info_raw, &info.color) {
        let raw_size = h as usize * linebytes_rounded(w, info.color.bpp_());
        converted = zero_vec(raw_size)?;
        lodepng_convert(&mut converted, image, &info.color, &state.info_raw, w, h)?;
        image = &converted;
    }
    add_chunk_idat(&mut outv, image, w, h, &info, &state.encoder, &state.encoder.zlibsettings)?;

    if info.time_defined {
        add_chunk_time(&mut outv, &info.time)?;
    }
    for t in &info.texts {
        if t.key.len() > 79 {
            return Err(Error::new(66));
        }
        if t.key.is_empty() {
            return Err(Error::new(67));
        }
        if state.encoder.text_compression {
            add_chunk_ztxt(&mut outv, &t.key, &t.value, &state.encoder.zlibsettings)?;
        } else {
            add_chunk_text(&mut outv, &t.key, &t.value)?;
        }
    }
    if state.encoder.add_id {
        let alread_added_id_text = info.texts.iter().any(|t| *t.key == b"LodePNG"[..]);
        if !alread_added_id_text {
            /*it's shorter as tEXt than as zTXt chunk*/
            add_chunk_text(&mut outv, b"LodePNG", LODEPNG_VERSION_STRING)?;
        }
    }
    for (k, l, t, s) in info.itext_keys() {
        if k.as_bytes().len() > 79 {
            return Err(Error::new(66));
        }
        if k.as_bytes().is_empty() {
            return Err(Error::new(67));
        }
        add_chunk_itxt(&mut outv, state.encoder.text_compression, k, l, t, s, &state.encoder.zlibsettings)?;
    }
    add_unknown_chunks(&mut outv, &info.unknown_chunks[ChunkPosition::IDAT as usize])?;
    add_chunk_iend(&mut outv)?;
    Ok(outv)
}

/*profile must already have been inited with mode.
It's ok to set some parameters of profile to done already.*/
/// basic flag is for internal use
#[inline(never)]
fn get_color_profile16(inp: &[u8], w: u32, h: u32, mode: &ColorMode) -> ColorProfile {
    let mut profile = ColorProfile::new();
    let numpixels: usize = w as usize * h as usize;
    let mut colored_done = mode.is_greyscale_type();
    let mut alpha_done = !mode.can_have_alpha();
    let bpp = mode.bpp_();
    let bytewidth = bpp.get() / 8;
    if bytewidth < 2 {
        return profile;
    }

    profile.bits = 16;
    /*counting colors no longer useful, palette doesn't support 16-bit*/
    for px in inp.chunks_exact(bytewidth as usize).take(numpixels) {
        let px = get_pixel_color_rgba16(px, mode);
        if !colored_done && (px.r != px.g || px.r != px.b) {
            profile.colored = true;
            colored_done = true;
        }
        if !alpha_done {
            let matchkey = px.r == profile.key_r && px.g == profile.key_g && px.b == profile.key_b;
            if px.a != 65535 && (px.a != 0 || (profile.key && !matchkey)) {
                profile.alpha = true;
                profile.key = false;
                alpha_done = true;
            } else if px.a == 0 && !profile.alpha && !profile.key {
                profile.key = true;
                profile.key_r = px.r;
                profile.key_g = px.g;
                profile.key_b = px.b;
            } else if px.a == 65535 && profile.key && matchkey {
                profile.alpha = true;
                profile.key = false;
                alpha_done = true;
            };
        }
        if alpha_done && colored_done {
            break;
        };
    }
    if profile.key && !profile.alpha {
        for px in inp.chunks_exact(bytewidth as usize).take(numpixels) {
            let px = get_pixel_color_rgba16(px, mode);
            if px.a != 0 && px.r == profile.key_r && px.g == profile.key_g && px.b == profile.key_b {
                profile.alpha = true;
                profile.key = false;
            }
        }
    }
    profile
}

// palette and gray < 8bit
#[inline(never)]
fn get_color_profile_low_bpp(inp: &[u8], w: u32, h: u32, mode: &ColorMode) -> ColorProfile {
    let numpixels: usize = w as usize * h as usize;
    let maxnumcolors = 1u16 << mode.bpp_().get();

    let mut used = [false; 256];
    let mut numcolors = 0;
    for i in 0..numpixels {
        let idx = get_pixel_low_bpp(inp, i, mode);
        if !used[idx as usize] {
            used[idx as usize] = true;
            numcolors += 1;
            if numcolors == maxnumcolors {
                break;
            }
        }
    }

    let mut gray_pal = [RGBA::new(0,0,0,0); 256];
    let palette = if mode.colortype == ColorType::PALETTE {
        mode.palette()
    } else {
        gray_palette(mode, &mut gray_pal)
    };

    let mut profile = ColorProfile::new();
    profile.bits = if mode.colortype == ColorType::PALETTE { 8 } else { 1 };
    profile.numcolors = numcolors;
    for ((i, px), _) in palette.iter().enumerate().zip(used).filter(|&(_, used)| used) {
        profile.palette[i] = *px;
        if profile.bits < 8 {
            let bits = get_value_required_bits(px.r);
            if bits > profile.bits {
                profile.bits = bits;
            }
        }
        if px.r != px.g || px.r != px.b {
            profile.colored = true;
        }
        if px.a != 255 {
            profile.alpha = true;
        }
    }

    if let Some((r,g,b)) = mode.key() {
        profile.key = true;
        profile.key_r = r;
        profile.key_g = g;
        profile.key_b = b;
    }
    profile
}

#[inline(never)]
pub(crate) fn get_color_profile(inp: &[u8], w: u32, h: u32, mode: &ColorMode) -> ColorProfile {
    let numpixels: usize = w as usize * h as usize;
    let bpp = mode.bpp_();
    let bytewidth = bpp.get() / 8;

    if mode.colortype == ColorType::PALETTE || bpp.get() < 8 || bytewidth == 0 {
        return get_color_profile_low_bpp(inp, w, h, mode);
    }

    /*Check if the 16-bit input is truly 16-bit*/
    if mode.bitdepth() == 16 && has_any_16_bit_pixels(inp, bytewidth, numpixels, mode) {
        return get_color_profile16(inp, w, h, mode);
    }

    let mut colored_done = mode.is_greyscale_type();
    let mut alpha_done = !mode.can_have_alpha();
    let mut numcolors_done = false;
    let mut bits_done = bpp.get() == 1;
    let maxnumcolors = 257;

    let mut profile = ColorProfile::new();
    let mut colormap = ColorIndices::with_capacity(maxnumcolors.into());
    for px in inp.chunks_exact(bytewidth as usize).take(numpixels) {
        let px = get_pixel_color_rgba8(px, mode);
        if !bits_done && profile.bits < 8 {
            let bits = get_value_required_bits(px.r);
            if bits > profile.bits {
                profile.bits = bits;
            };
        }
        bits_done = profile.bits >= bpp.get();
        if !colored_done && (px.r != px.g || px.r != px.b) {
            profile.colored = true;
            colored_done = true;
            if profile.bits < 8 {
                profile.bits = 8;
            };
            /*PNG has no colored modes with less than 8-bit per channel*/
        }
        if !alpha_done && profile.check_alpha(px){
            alpha_done = true;
        }
        if !numcolors_done && colormap.get(&px).is_none() {
            colormap.insert(px, profile.numcolors as u8);
            if profile.numcolors < 256 {
                profile.palette[profile.numcolors as usize] = px;
            }
            profile.numcolors += 1;
            numcolors_done = profile.numcolors >= maxnumcolors;
        }
        if alpha_done && numcolors_done && colored_done && bits_done {
            break;
        };
    }
    if profile.key && !profile.alpha {
        for px in inp.chunks_exact(bytewidth as usize).take(numpixels) {
            let px = get_pixel_color_rgba8(px, mode);
            if px.a != 0 && px.r as u16 == profile.key_r && px.g as u16 == profile.key_g && px.b as u16 == profile.key_b {
                profile.alpha = true;
                profile.key = false;
                /*PNG has no alphachannel modes with less than 8-bit per channel*/
                if profile.bits < 8 {
                    profile.bits = 8;
                }
                break;
            }
        }
    }
    /*make the profile's key always 16-bit for consistency - repeat each byte twice*/
    profile.key_r += profile.key_r << 8;
    profile.key_g += profile.key_g << 8;
    profile.key_b += profile.key_b << 8;
    profile
}

fn has_any_16_bit_pixels(inp: &[u8], bytewidth: u8, numpixels: usize, mode: &ColorMode) -> bool {
    if bytewidth == 0 { debug_assert!(false); return false; }
    for px in inp.chunks_exact(bytewidth as usize).take(numpixels) {
        let px = get_pixel_color_rgba16(px, mode);
        if px.as_slice().iter().any(|c| (c >> 8) != (c & 0xFF)) {
            return true;
        }
    }
    false
}


/*Automatically chooses color type that gives smallest amount of bits in the
output image, e.g. grey if there are only greyscale pixels, palette if there
are less than 256 colors, …
Updates values of mode with a potentially smaller color model. mode_out should
contain the user chosen color model, but will be overwritten with the new chosen one.*/
#[inline(never)]
pub(crate) fn auto_choose_color(image: &[u8], w: u32, h: u32, mode_in: &ColorMode) -> Result<ColorMode, Error> {
    let mut mode_out = ColorMode::new();
    let mut prof = get_color_profile(image, w, h, mode_in);

    mode_out.clear_key();
    if prof.key && w * h <= 16 {
        prof.alpha = true;
        prof.key = false;
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
        (w as usize * h as usize >= (n * 2) as usize) &&
        (prof.colored || prof.bits > palettebits);
    if palette_ok {
        let pal = &prof.palette[0..prof.numcolors as usize];
        /*remove potential earlier palette*/
        mode_out.palette_clear();
        for p in pal {
            mode_out.palette_add(*p)?;
        }
        mode_out.colortype = ColorType::PALETTE;
        mode_out.try_set_bitdepth(palettebits.into())?;
        if mode_in.colortype == ColorType::PALETTE && mode_in.palette().len() >= mode_out.palette().len() && mode_in.bitdepth == mode_out.bitdepth {
            /*If input should have same palette colors, keep original to preserve its order and prevent conversion*/
            mode_out = mode_in.clone();
        };
    } else {
        mode_out.try_set_bitdepth(prof.bits.into())?;
        mode_out.colortype = if prof.alpha {
            if prof.colored {
                ColorType::RGBA
            } else {
                ColorType::GREY_ALPHA
            }
        } else if prof.colored {
            ColorType::RGB
        } else {
            ColorType::GREY
        };
        if prof.key {
            let mask = ((1 << mode_out.bitdepth()) - 1) as u16;
            /*profile always uses 16-bit, mask converts it*/
            mode_out.set_key(
                prof.key_r & mask,
                prof.key_g & mask,
                prof.key_b & mask);
        };
    }
    Ok(mode_out)
}


#[inline]
pub(crate) fn lodepng_encode_memory(image: &[u8], w: u32, h: u32, colortype: ColorType, bitdepth: u32) -> Result<Vec<u8>, Error> {
    let mut state = Encoder::new();
    state.info_raw_mut().colortype = colortype;
    state.info_raw_mut().try_set_bitdepth(bitdepth)?;
    state.info_png_mut().color.colortype = colortype;
    state.info_png_mut().color.try_set_bitdepth(bitdepth)?;
    lodepng_encode(image, w as _, h as _, &state.state)
}

impl EncoderSettings {
    unsafe fn predefined_filters(&self, len: usize) -> Result<&[u8], Error> {
        if self.predefined_filters.is_null() {
            Err(Error::new(1))
        } else {
            Ok(slice::from_raw_parts(self.predefined_filters, len))
        }
    }
}

impl ColorProfile {
    #[must_use]
    pub fn new() -> Self {
        Self {
            colored: false,
            key: false,
            key_r: 0,
            key_g: 0,
            key_b: 0,
            alpha: false,
            numcolors: 0,
            bits: 1,
            palette: [RGBA{r:0,g:0,b:0,a:0}; 256],
        }
    }

    // true if done checking
    fn check_alpha(&mut self, px: RGBA) -> bool {
        let matchkey = px.r as u16 == self.key_r && px.g as u16 == self.key_g && px.b as u16 == self.key_b;
        if px.a != 255 && (px.a != 0 || (self.key && !matchkey)) {
            self.alpha = true;
            self.key = false;
            /*PNG has no alphachannel modes with less than 8-bit per channel*/
            if self.bits < 8 {
                self.bits = 8;
            }
            return true;
        } else if px.a == 0 && !self.alpha && !self.key {
            self.key = true;
            self.key_r = px.r as u16;
            self.key_g = px.g as u16;
            self.key_b = px.b as u16;
        } else if px.a == 255 && self.key && matchkey {
            self.alpha = true;
            self.key = false;
            if self.bits < 8 {
                self.bits = 8;
            };
            return true;
        }
        false
    }
}

/*Returns how many bits needed to represent given value (max 8 bit)*/
fn get_value_required_bits(value: u8) -> u8 {
    match value {
        0 | 255 => 1,
        x if x % 17 == 0 => {
            /*The scaling of 2-bit and 4-bit values uses multiples of 85 and 17*/
            if value % 85 == 0 { 2 } else { 4 }
        },
        _ => 8,
    }
}

unsafe impl Send for CompressSettings {}
unsafe impl Sync for CompressSettings {}
unsafe impl Sync for DecompressSettings {}
unsafe impl Send for DecompressSettings {}
