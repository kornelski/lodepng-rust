use png::Transformations;
use rgb::ComponentBytes;
extern crate lodepng;

fn main() {
    let data = std::fs::read(std::env::args().nth(1).unwrap()).unwrap();

    if data.len() < 4 {
        return;
    }
    let width = (data[0] as usize).min(data.len()).max(1);
    let height = data.len()/width;
    let data = &data[..width * height];

    let lode_encoded = lodepng::encode_memory(data, width, height, lodepng::ColorType::GREY, 8).unwrap();

    let mut other = Vec::new();
    let mut encoder = png::Encoder::new(&mut other, width.try_into().unwrap(), height.try_into().unwrap());
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    let mut encoder = encoder.write_header().unwrap();
    encoder.write_image_data(data).unwrap();
    drop(encoder);

    let mut for_reading = lode_encoded.as_slice();
    let mut decoder = png::Decoder::new(&mut for_reading);
    decoder.set_transformations(Transformations::IDENTITY);

    let mut reader = decoder.read_info().unwrap();
    assert_eq!((png::ColorType::Grayscale, png::BitDepth::Eight), reader.output_color_type());
    // Allocate the output buffer.
    let mut buf = vec![0; reader.output_buffer_size()];
    // Read the next frame. An APNG might contain multiple frames.
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.width, width.try_into().unwrap());
    assert_eq!(info.height, height.try_into().unwrap());
    // Grab the bytes of the image.
    let decoded_bytes = &buf[..info.buffer_size()];
    assert_eq!(data, decoded_bytes);

    std::fs::write(format!("/tmp/test-{width}x{height}-expected.png"), other).unwrap();
    let name = format!("/tmp/test-{width}x{height}-actual.png");
    eprintln!("{name}");
    std::fs::write(name, &lode_encoded).unwrap();
    let decoded = lodepng::decode_memory(lode_encoded, lodepng::ColorType::GREY, 8).unwrap();

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
}
