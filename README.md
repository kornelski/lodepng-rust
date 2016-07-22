#[LodePNG](http://lodev.org/lodepng/) bindings for [Rust](http://www.rust-lang.org/)

LodePNG is a stand-alone PNG image decoder and encoder (does *not* require zlib nor libpng).

This package allows easy reading and writing of PNG files without any system dependencies.

The easiest way to use LodePNG is to simply include the [lodepng crate](https://crates.io/crates/lodepng).
To do so, add this to your `Cargo.toml`:

```toml
[dependencies]
lodepng = "0.11"
```

## API

See [API documentation](http://pornel.github.io/lodepng-rust/lodepng/) for details. The API mimics lodepng, so if something is unclear, [see the original lodepng.h](http://lpi.googlecode.com/svn/trunk/lodepng.h).

To load RGBA PNG file:

```rust
lodepng::decode32_file("in.png")
```

returns `lodepng::Bitmap<lodepng::RGBA<u8>>` with `.width`, `.height`, and `.buffer`. 

The RGB/RGBA pixel types are from the [RGB create](https://crates.io/crates/rgb), which you can import separately to use the same pixel struct throughout the program, without casting.

To save an RGBA PNG file:

```rust
lodepng::encode32_file("out.png", &buffer, width, height)
```

### Advanced

```rust
let mut state = lodepng::State::new();
state.remember_unknown_chunks(true);

match state.decode("in.png") {
    Ok(lodepng::Image::RGB(image)) => {…}
    Ok(lodepng::Image::RGBA(image)) => {…}
    Ok(lodepng::Image::RGBA16(image)) => {…}
    Ok(lodepng::Image::Gray(image)) => {…}
    Ok(_) => {…}
    Err(err) => {…}
}

for chunk in state.info_png().unknown_chunks() {
    println!("{:?} = {:?}", chunk.name(), chunk.data());
}

// Color profile
let icc_data = state.info_png().get_icc();
```
