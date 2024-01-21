use std::sync::mpsc;
use lodepng::FilterStrategy;
use std::path::Path;


fn main() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(debug_assertions) {
        eprintln!("warning: this example should be built in release mode (cargo run --release)");
    }
    if cfg!(not(any(feature = "cfzlib", feature = "ngzlib"))) {
        eprintln!("warning: build with --features=cfzlib or --features=ngzlib for better results");
    }

    let path = std::env::args().nth(1).ok_or("Specify a path to a PNG file")?;

    let source_png = std::fs::read(&path).map_err(|e| format!("Can't load {path}: {e}"))?;
    let source_len = source_png.len();
    let mut decoder = lodepng::Decoder::new();
    let img = decoder.decode(source_png).map_err(|e| format!("Can't decode {path}: {e}"))?;
    println!("Original size: {} bytes ({}x{} {:?})", source_len, img.width(), img.height(), decoder.info_raw().colortype());

    let mut encoder = lodepng::Encoder::new();
    encoder.set_auto_convert(true);
    encoder.settings_mut().set_level(9);
    *encoder.info_raw_mut() = decoder.info_raw().clone();
    encoder.info_png_mut().color = decoder.info_raw().clone();

    let (tx, rx) = mpsc::channel();
    let img = &img;
    let encoder = &encoder;
    std::thread::scope(|s| {
        for strategy in [FilterStrategy::ZERO, FilterStrategy::MINSUM, FilterStrategy::ENTROPY, FilterStrategy::BRUTE_FORCE] {
            let tx = tx.clone();
            s.spawn(move || {
                let mut encoder = encoder.clone();
                encoder.set_filter_strategy(strategy, false);
                let new_png = encoder.encode(img.bytes(), img.width(), img.height())?;
                tx.send((strategy, new_png)).unwrap();
                Ok::<_, lodepng::Error>(())
            });
        }
        drop(tx);
    });

    let (strategy, new_png) = rx.into_iter().inspect(|(strategy, new_png)| {
        println!("New png size: {} bytes ({strategy:?})", new_png.len());
    }).min_by_key(|(_, a)| a.len()).unwrap();

    let file_name = Path::new(&path).file_stem().and_then(|f| f.to_str()).ok_or("Invalid path")?;
    let new_file_name = format!("{}-optimized.png", file_name);
    if new_png.len() < source_len && !Path::new(&new_file_name).exists() {
        std::fs::write(&new_file_name, new_png)?;
        println!("Wrote optimized PNG to {} (best strategy: {strategy:?})", new_file_name);
    } else {
        println!("Strategy {strategy:?} was most effective.");
    }
    Ok(())
}
