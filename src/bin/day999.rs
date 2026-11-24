use std::{env, fs, hint::black_box, io};

fn main() -> io::Result<()> {
    let path = env::args_os().nth(1).expect("pass the fake input path");
    let input = fs::read(path)?;
    black_box(Box::leak(input.into_boxed_slice()));
    Ok(())
}
