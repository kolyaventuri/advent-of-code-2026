use std::env;
use std::fs;
use std::path::PathBuf;

pub fn read_input(day: u32) -> String {
    let mut args = env::args_os().skip(1);
    let filename = if args.any(|arg| arg == "--test") {
        args.next()
            .filter(|arg| !arg.as_encoded_bytes().starts_with(b"-"))
            .unwrap_or_else(|| format!("day{day}.test.txt").into())
    } else {
        format!("day{day}.txt").into()
    };

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("input")
        .join(filename);

    fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!("Failed to read {}: {err}", path.display())
    })
}
