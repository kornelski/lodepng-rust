#![no_main]
use rgb::ComponentBytes;
#[macro_use] extern crate libfuzzer_sys;
extern crate lodepng;

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }
    let width = (data[0] as usize).min(data.len()).max(1);
    let height = data.len()/width;
    let data = &data[..width * height];

    let file = lodepng::encode_memory(data, width, height, lodepng::ColorType::GREY, 8).unwrap();
    let decoded = lodepng::decode_memory(file, lodepng::ColorType::GREY, 8).unwrap();

    match decoded {
        lodepng::Image::RawData(_) => panic!("unexpected RawData"),
        lodepng::Image::Grey(img) => {
            assert_eq!(img.width, width);
            assert_eq!(img.height, height);
            assert_eq!(data, img.buffer.as_slice().as_bytes());
        },
        lodepng::Image::Grey16(_) => panic!("unexpected Grey16"),
        lodepng::Image::GreyAlpha(_) => panic!("unexpected GreyAlpha"),
        lodepng::Image::GreyAlpha16(_) => panic!("unexpected GreyAlpha16"),
        lodepng::Image::RGBA(_) => panic!("unexpected RGBA"),
        lodepng::Image::RGB(_) => panic!("unexpected RGB"),
        lodepng::Image::RGBA16(_) => panic!("unexpected RGBA16"),
        lodepng::Image::RGB16(_) => panic!("unexpected RGB16"),
    }
});
