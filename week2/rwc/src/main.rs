use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Too few arguments.");
        process::exit(1);
    }
    let filename = &args[1];

    let file = File::open(filename).expect(&format!("Invalid filename: {filename}"));
    let mut lines: usize = 0;
    let words: usize = BufReader::new(file)
        .lines()
        .map(|line| {
            lines += 1;
            line.unwrap()
                .split_ascii_whitespace()
                .collect::<Vec<&str>>()
                .len()
        })
        .sum();

    println!(
        "{lines} {words} {} {filename}",
        fs::metadata(filename).unwrap().len()
    );
}
