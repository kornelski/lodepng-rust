#![no_main]
use rgb::ComponentBytes;
#[macro_use] extern crate libfuzzer_sys;
extern crate lodepng;

fuzz_target!(|data: &[u8]| {
    if data.len() < 5 {
        return;
    }
    let (seed, data) = data.split_first().unwrap();
    let bytes_per_pixel = 3;
    let max_width = (*seed as usize).min(data.len()/bytes_per_pixel);
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
    encoder.info_raw_mut().set_colortype(lodepng::ColorType::RGB);

    let file = encoder.encode(data, width, height).unwrap();
    let img = lodepng::decode24(file).unwrap();

    assert_eq!(img.width, width);
    assert_eq!(img.height, height);
    assert_eq!(data, img.buffer.as_slice().as_bytes());
});
