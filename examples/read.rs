extern crate lodepng;

fn main() {
    match lodepng::decode32_file("tests/test.png") {
        Ok(bitmap) => println!("Decoded image {} x {} and the first pixel's value is {}",
                                bitmap.width, bitmap.height, bitmap.buffer.as_ref()[0]),
        Err(reason) => println!("Could not load, because: {}", reason),
    }
}
