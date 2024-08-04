use png::BitDepth;
use rgb::*;
use std::fs::*;
use std::path::*;
use rgb::bytemuck::cast_slice;

fn decode(path: &Path) -> Vec<RGBA8> {
    let mut p = png::Decoder::new(File::open(path).unwrap());
    p.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = p.read_info().unwrap();
    let mut data = vec![0u8; reader.output_buffer_size()];
    reader.next_frame(&mut data).unwrap();

    match reader.output_color_type() {
        (png::ColorType::Rgba, BitDepth::Eight) => cast_slice::<_, RGBA8>(&data).to_owned(),
        (png::ColorType::Rgb, BitDepth::Eight) => cast_slice::<_, RGB8>(&data).iter().map(|&p| p.with_alpha(255)).collect(),
        (png::ColorType::Grayscale, BitDepth::Eight) => data.iter().map(|&p| RGBA8::new(p,p,p,255)).collect(),
        (png::ColorType::GrayscaleAlpha, BitDepth::Eight) => data.chunks(2).map(|c| RGBA8::new(c[0],c[0],c[0],c[1])).collect(),
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

        if file_name.starts_with('x') {
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
