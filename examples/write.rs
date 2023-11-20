use std::path::Path;

fn main() {
    let path = Path::new("example_8bpp.png");

    let pixels = [[255u8, 0, 0],   [0, 255, 0],
                 [0, 0, 255],   [0, 99, 99]];

    // encode_file takes the path to the image, an array of pixls (can be flat u8, or item per pixel),
    // the width, the height, the color mode, and the bit depth
    if let Err(e) = lodepng::encode_file(path, &pixels, 2, 2, lodepng::ColorType::RGB, 8) {
        panic!("failed to write {}: {e}", path.display());
    }

    println!("Written to {}", path.display());


    let path = Path::new("example_1bpp.png");

    let pixels = [
        0b1010_1010u8,
        0b0101_0101,
        0b1010_1010,
        0b0101_0101,
    ];

    if let Err(e) = lodepng::encode_file(path, &pixels, 8, 4, lodepng::ColorType::GREY, 1) {
        panic!("failed to write {}: {e}", path.display());
    }

    println!("Written to {}", path.display());
}
