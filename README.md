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

The API mimics lodepng, so for full documentation of the structures and functions, [see the original lodepng.h](http://lpi.googlecode.com/svn/trunk/lodepng.h).

To load RGBA PNG file:

```rust
lodepng::decode32_file(&Path::new("in.png"))
```

returns `lodepng::RawBitmap` with `.width`, `.height` and `u8` `.buffer`.

To save RGBA PNG file:

```rust
lodepng::encode32_file(&Path::new("out.png"), buffer.as_slice(), width, height)
```

If you'd rather work with RGBA structure than u8 arrays, here's a handy function:

```rust
#[repr(C)]
#[deriving(Clone)]
struct RGBA {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

fn convert(bytes: &mut [u8]) -> &mut [RGBA] {
    unsafe {
        std::mem::transmute(std::raw::Slice {
            data: bytes.as_mut_ptr() as *const RGBA,
            len: bytes.len() / 4,
        })
    }
}
```
