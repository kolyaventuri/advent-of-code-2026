/**
 * Re-implementation of 2025's Day 1 as practice
 */
use std::{error::Error, process::ExitCode};
use aoc26::read_input;

fn run() -> Result<(), Box<dyn Error>> {
    let input = read_input(0);

    let (part1, part2) = solve(&input)?;
    println!("Part 1: {part1}");
    println!("Part 2: {part2}");
    Ok(())
}

fn solve(input: &str) -> Result<(i64, i64), Box<dyn Error>> {
    let mut part1 = 0;
    let mut part2 = 0;

    let mut dir = 0;
    let mut current = 50;

    for line in input.lines() {
        if &line[..1] == "L" {
            dir = -1;
        } else if &line[..1] == "R" {
            dir = 1;
        }

        let number = &line[1..].parse::<i64>()?;
        let n =  number % 100;
        let mut extra = (number - n) / 100;

        let last = current;
        current += dir * n;

        if last > 0 && current <= 0 {
            extra += 1;
        }

        if current < 0 {
            current += 100;
        } else if current > 99 {
            current -= 100;
            extra += 1;
        }

        part2 += extra;
        if current == 0 { part1 += 1; }
    }

    return Ok((part1, part2));

}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("day0: {error}");
            ExitCode::FAILURE
        }
    }
}
