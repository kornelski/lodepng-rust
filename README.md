#[LodePNG](http://lodev.org/lodepng/) bindings for [Rust](http://www.rust-lang.org/)

LodePNG is a stand-alone PNG image decoder and encoder (does *not* require zlib or libpng).

This package allows easy reading and writing of PNG files without any system dependencies.

The easiest way to use LodePNG is to simply include the lodepng crate.
To do so, add this to your Cargo.toml:

```toml
[dependencies.lodepng]
git = "https://github.com/pornel/lodepng-rust.git"
```

To build the `lodepng` crate:

```sh
cargo build
```

It will produce `liblodepng-….rlib` that you can import with `extern crate lodepng`.

## API

See [API documentation](http://pornel.github.io/lodepng-rust/lodepng/) for details. The API mimics lodepng, so if something is unclear, [see the original lodepng.h](http://lpi.googlecode.com/svn/trunk/lodepng.h).

To load RGBA PNG file:

```rust
lodepng::decode32_file("in.png")
```

returns `lodepng::Bitmap<lodepng::RGBA<u8>>` with `.width`, `.height`, and `.buffer`.

To save RGBA PNG file:

```rust
lodepng::encode32_file("out.png", buffer.as_slice(), width, height)
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
```
