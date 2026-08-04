mod image;

use std::{env, process};

fn main() {
    if let Err(error) = run() {
        eprintln!("ERROR {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let command = args.next().ok_or_else(usage)?;
    if args.next().is_some() {
        return Err(usage());
    }

    match command.as_str() {
        "build" => {
            let _images = image::build_images()?;
            image::report_build_success();
            Ok(())
        }
        _ => Err(usage()),
    }
}

fn usage() -> String {
    "usage: cargo xtask build".to_owned()
}
