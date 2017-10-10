extern crate lodepng;

fn main() {
    match lodepng::decode32_file("tests/test.png") {
        Ok(image) => println!("Decoded image {} x {} and the first pixel's value is {}",
                                image.width, image.height, image.buffer[0]),
        Err(reason) => println!("Could not load, because: {}", reason),
    }
}
