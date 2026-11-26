# Advent of Code 2026

## Run a puzzle

```sh
cargo run --bin day0 -- --test
cargo run --bin day0
```

Run from the repository root. The `--test` flag reads `input/dayN.test.txt` by default.
Pass a filename after it to select another file in `input/`, for example
`cargo run --bin day0 -- --test day0.test2.txt`.

## Generate a report

On macOS, use the built-in `/usr/bin/time` and `/usr/bin/leaks`:

```sh
cargo aoc-report baseline
cargo aoc-report 0
```

The timing report shows the median wall-clock time and middle 50% of 15 runs
after two untimed warmups. Each run launches the release binary. Use
`cargo aoc-report 0 --runs 7` to change the number of timed runs. The command
prints progress to stderr as it builds, runs measurements, and checks for leaks.

## Pre-commit hooks

A pre-commit hook (`git config core.hooksPath .githooks`) exists to ensure that puzzle inputs are not included in commits. It is not comprehensive, but it generally catches anything that might leak.

## AI disclosure

AI assistance was used for the Cargo tooling, Day 0 template, report generator + sanitization, hook, and some documentation. Puzzle analysis and solutions are done manually.
