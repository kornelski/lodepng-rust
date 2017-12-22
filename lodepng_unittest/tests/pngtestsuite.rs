extern crate png;
extern crate lodepng;
extern crate glob;
extern crate rgb;
use rgb::*;
use png::HasParameters;
use std::path::*;
use std::fs::*;
fn decode(path: &Path) -> Vec<RGBA8> {
    let mut p = png::Decoder::new(File::open(path).unwrap());
    p.set(png::Transformations::EXPAND);
    let (info, mut reader) = p.read_info().unwrap();
    let mut data = vec![0u8; info.buffer_size()];
    reader.next_frame(&mut data).unwrap();

    match info.color_type {
        png::ColorType::RGBA => data.as_rgba().to_owned(),
        png::ColorType::RGB => data.as_rgb().iter().map(|&p| p.alpha(255)).collect(),
        png::ColorType::Grayscale => data.iter().map(|&p| RGBA::new(p,p,p,255)).collect(),
        png::ColorType::GrayscaleAlpha => data.chunks(2).map(|c| RGBA::new(c[0],c[0],c[0],c[1])).collect(),
        _ => panic!(),
    }
}

#[test]
fn test_pngtestsuite() {
    for file in glob::glob("tests/pngtestsuite/*.png").unwrap() {
        let file = file.unwrap();
        let file_name = file.file_name().unwrap().to_str().unwrap();

        if file_name.ends_with("16.png") { // couldn't get piston to decode 16-bit
            continue;
        }

        if file_name.starts_with("x") {
            if file_name.starts_with("xcs") { // just checksum, meh
                continue;
            }
            assert!(lodepng::decode32_file(&file).is_err(), "should fail: {}", file.display());
        } else {
            let a = lodepng::decode32_file(&file).expect(file.to_str().unwrap());
            let b = decode(&file);
            assert_eq!(a.buffer, b, "should equal {}", file.display());
        }
    }
}
