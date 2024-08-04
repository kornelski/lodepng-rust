use rgb::bytemuck::cast_slice;
use std::path::Path;

fn main() {
    let path = Path::new("tests/test.png");

    let mut state = lodepng::Decoder::new();

    match state.decode_file(path) {
        Ok(image) => match image {
            lodepng::Image::RGBA(bitmap) => {
                println!("Decoded image {} x {}", bitmap.width, bitmap.height);
                println!("The first pixel is {}", bitmap.buffer[0]);
                println!("The raw bytes are {:?}", cast_slice::<_, u8>(bitmap.buffer.as_slice()));
            },
            x => println!("Decoded some other image format {x:?}"),
        },
        Err(reason) => println!("Could not load {}, because: {}", path.display(), reason),
    }
}
