use lodepng::*;
use rgb::bytemuck::cast_slice;

#[test]
fn roundtrip_grey() {
    roundtrip_color(ColorType::GREY, &[8, 16]);
}

#[test]
fn roundtrip_rgb() {
    roundtrip_color(ColorType::RGB, &[8, 16]);
}

#[test]
fn roundtrip_rgba() {
    roundtrip_color(ColorType::RGBA, &[8, 16]);
}

#[test]
fn roundtrip_grey_alpha() {
    roundtrip_color(ColorType::GREY_ALPHA, &[8, 16]);
}

#[track_caller]
fn roundtrip_color(colortype: ColorType, bitdepths: &[u32]) {
    let filter_strategies = [FilterStrategy::ZERO, FilterStrategy::MINSUM, FilterStrategy::ENTROPY, FilterStrategy::ENTROPY, FilterStrategy::BRUTE_FORCE];
    let mut n=0;
    let mut data = vec![0; 256 + 256*256*colortype.bpp(16) as usize];
    for &bitdepth in bitdepths {
        for width in [1,2,3,4,5,6,7,8,9,15,16,17,64,127,256] {
            randomize(&mut data);
            for height in [1,2,3,4,5,6,7,8,9,15,16,17,64,127,256] {
                for interlace in [0, 1] {
                    for filters in [false, true] {
                        let (filters, data) = if filters {
                            let (f, data) = data.split_at(height);
                            (Some(f.iter().map(|&f| f%5).collect()), data)
                        } else {
                            (None, &data[..])
                        };
                        n += 1;
                        rountrip_data(data, width, height, interlace, colortype, bitdepth, filters, filter_strategies[n % filter_strategies.len()]);
                    }
                }
            }
        }
    }
}

fn randomize(data: &mut [u8]) {
    let mut seed = u32::from(data[0]);
    for b in data {
        seed = 1103515245u32.wrapping_mul(seed).wrapping_add(12345);
        *b ^= (seed >> 17) as u8;
    }
}

#[track_caller]
fn rountrip_data(data: &[u8], width: usize, height: usize, interlace: u8, colortype: ColorType, bitdepth: u32, filters: Option<Box<[u8]>>, filter_strategy: FilterStrategy) {
    let bytes_per_pixel = colortype.bpp(bitdepth) as usize / 8;
    let data = &data[..(width * height)*bytes_per_pixel];

    let mut encoder = lodepng::Encoder::new();
    encoder.set_auto_convert(false);
    if let Some(f) = filters {
        encoder.set_predefined_filters(f);
    } else {
        encoder.set_filter_strategy(filter_strategy, false);
    }
    encoder.info_raw_mut().set_bitdepth(bitdepth);
    encoder.info_raw_mut().set_colortype(colortype);
    encoder.info_png_mut().color.set_bitdepth(bitdepth);
    encoder.info_png_mut().color.set_colortype(colortype);
    encoder.info_png_mut().interlace_method = interlace;

    let file = encoder.encode(data, width, height).unwrap();
    let img = lodepng::decode_memory(file, colortype, bitdepth).unwrap();
    match img {
        Image::RGB(img) => assert_img_equals(&img, cast_slice(&img.buffer), width, height, data),
        Image::RGBA(img) => assert_img_equals(&img, cast_slice(&img.buffer), width, height, data),
        Image::GreyAlpha(img) => assert_img_equals(&img, cast_slice(&img.buffer), width, height, data),
        Image::Grey(img) => assert_img_equals(&img, cast_slice(&img.buffer), width, height, data),
        Image::RawData(_) => unreachable!(),
        Image::Grey16(img) => assert_img_equals(&img, cast_slice(&img.buffer), width, height, data),
        Image::GreyAlpha16(img) => assert_img_equals(&img, cast_slice(&img.buffer), width, height, data),
        Image::RGBA16(img) => assert_img_equals(&img, cast_slice(&img.buffer), width, height, data),
        Image::RGB16(img) => assert_img_equals(&img, cast_slice(&img.buffer), width, height, data),

    }
}

#[track_caller]
fn assert_img_equals<T>(img: &Bitmap<T>, buffer: &[u8], width: usize, height: usize, data: &[u8]) {
    assert_eq!(img.width, width);
    assert_eq!(img.height, height);
    assert_eq!(data, buffer);
}
