#![no_main]
use rgb::ComponentBytes;
#[macro_use] extern crate libfuzzer_sys;

use lodepng::*;

fuzz_target!(|data: &[u8]| {
    if data.len() < 5 {
        return;
    }
    let (seed, data) = data.split_at(2);
    let interlace = data.last().copied().unwrap_or(0) & 1;
    let (bytes_per_pixel, colortype) = match 1 + seed[1] as usize % 4 {
        4 => (4, lodepng::ColorType::RGBA),
        2 => (2, lodepng::ColorType::GREY_ALPHA),
        1 => (1, lodepng::ColorType::GREY),
        _ => (3, lodepng::ColorType::RGB),
    };
    let width = seed[0] as usize + if seed[0] > 200 { seed[1] as usize * 2 } else { 1 };
    let max_width = (width).min(data.len()/bytes_per_pixel);
    if max_width < 1 {
        return;
    }
    let height = data.len()/(max_width*bytes_per_pixel);
    let (filters, data) = data.split_at(height);
    let mut filters: Box<[u8]> = filters.into();
    filters.iter_mut().for_each(|f| *f = *f%5);

    let width = data.len()/(height*bytes_per_pixel);
    if width < 1 {
        return;
    }

    let data = &data[..(width * height)*bytes_per_pixel];
    assert_eq!(0, data.len()%bytes_per_pixel);

    let mut encoder = lodepng::Encoder::new();
    encoder.set_auto_convert(false);
    encoder.set_predefined_filters(filters);
    encoder.info_raw_mut().set_bitdepth(8);
    encoder.info_raw_mut().set_colortype(colortype);
    encoder.info_png_mut().interlace_method = interlace;

    let file = encoder.encode(data, width, height).unwrap();
    let img = lodepng::decode_memory(file, colortype, 8).unwrap();
    match img {
        Image::RGB(img) => {
            assert_eq!(img.width, width);
            assert_eq!(img.height, height);
            assert_eq!(data, img.buffer.as_slice().as_bytes());
        },
        Image::RGBA(img) => {
            assert_eq!(img.width, width);
            assert_eq!(img.height, height);
            assert_eq!(data, img.buffer.as_slice().as_bytes());
        },
        Image::GreyAlpha(img) => {
            assert_eq!(img.width, width);
            assert_eq!(img.height, height);
            assert_eq!(data, img.buffer.as_slice().as_bytes());
        },
        Image::Grey(img) => {
            assert_eq!(img.width, width);
            assert_eq!(img.height, height);
            assert_eq!(data, img.buffer.as_slice().as_bytes());
        },
        _ => unreachable!(),
    }
});
