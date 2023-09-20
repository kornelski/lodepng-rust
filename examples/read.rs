
fn main() {
    let arg = std::env::args().nth(1);
    let path = arg.as_deref().unwrap_or("tests/test.png");

    match lodepng::decode32_file(path) {
        Ok(image) => println!("Decoded image {} x {} and the first pixel's value is {}",
                                image.width, image.height, image.buffer[0]),
        Err(reason) => println!("Could not load, because: {reason}"),
    }
}
