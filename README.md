# Pizza CLI & Core Library

A small Rust workspace for calculating pizza dough recipes and fermentation timelines.  
This project started as a playful experiment and gradually became a reusable library (`pizza-core`) plus a command-line tool (`pizza-cli`).

The goal is to provide bakers and hobbyists with a way to compute ingredient weights and timelines given flour strength, hydration, temperature, yeast type, and fermentation strategy (with or without fridge).

---

## Table of Contents

- [Overview](#overview)
- [How the calculations work](#how-the-calculations-work)
  - [Ingredients math](#ingredients-math)
  - [Yeast models](#yeast-models)
  - [Effective fermentation hours](#effective-fermentation-hours)
  - [Timelines](#timelines)
- [Compiling and running](#compiling-and-running)
  - [Requirements](#requirements)
  - [Build](#build)
  - [Run](#run)
  - [Examples](#examples)
- [Project structure](#project-structure)
- [Contributing](#contributing)
- [License](#license)

---

## Overview

The workspace has two crates:

- **`pizza-core`**: a pure library with all the formulas, unit-tested.
- **`pizza-cli`**: a command-line application that uses `pizza-core`, handles JSON profiles, pretty tables, and time-of-day calculations.

The project is open source, built for fun, and intended for learning and experimentation.  
Do not treat the output as professional baking advice: the numbers are heuristics and approximations.

---

## How the calculations work

### Ingredients math

Given:
- total dough weight = number of balls × weight per ball,
- hydration as a fraction (e.g. 0.75 = 75%),
- salt expressed in g/kg of flour,
- yeast type and estimated percentage.

The formulas are:

- **Baker’s yeast (dry or fresh)**  
  ```
  flour = total_dough / (1 + hydration + salt% + yeast%)
  water = flour × hydration
  salt  = flour × salt%
  yeast = flour × yeast%
  ```

### Yeast models

- **Dry yeast baseline**: 0.35% of flour at 25 °C, W=260, 12h.  
  Adjustments:
  - Temperature: Q10 ≈ 2 per 10 °C difference.
  - Flour strength (W): mild effect (exponent 0.2).
  - Time: inversely proportional.

- **Fresh yeast**: treated as ~3× dry yeast.

### Effective fermentation hours

Fridge fermentation is slower. We model this with a **fridge factor** (default 0.25):

```
effective_hours = (total_hours - fridge_hours) + fridge_hours × fridge_factor
```

So 4h in fridge counts like 1h at room temperature.

### Timelines

Two modes:

- **No fridge**: total time split ~55% bulk, ~45% final proof, adjusted by temperature.
- **With fridge**:  
  ```
  total = bulk + fridge + warmup + proof
  ```
  Remaining time (after fridge+warmup) is split ~35% bulk / ~65% proof, adjusted by temperature.

---

## Compiling and running

### Requirements

- Rust toolchain (1.70+ recommended).  
  Install via [rustup](https://rustup.rs).

### Build

Clone and build the workspace:

```bash
git clone https://github.com/marcocot/pizza-cli.git
cd pizza-workspace
cargo build --release
```

### Run

Run the CLI directly:

```bash
cargo run -p pizza-cli -- --w 270 --temp 25 --yeast dry   --hydration 0.75 --ball-weight 280 --balls 2   --salt-per-kg 20 --total-hours 11 --start 09:00
```

### Examples

- **Dry yeast, no fridge**:
```bash
cargo run -p pizza-cli -- --w 270 --temp 25 --yeast dry   --hydration 0.75 --ball-weight 280 --balls 2   --salt-per-kg 20 --total-hours 11 --start 09:00
```

- **Fresh yeast with fridge**:
```bash
cargo run -p pizza-cli -- --w 270 --temp 24 --yeast fresh   --hydration 0.70 --ball-weight 260 --balls 4   --salt-per-kg 22 --total-hours 24   --fridge-hours 16 --warmup-hours 3 --fridge-factor 0.25   --start 18:00
```

- **Save a profile**:
```bash
cargo run -p pizza-cli -- --w 270 --temp 25 --yeast dry   --hydration 0.75 --ball-weight 280 --balls 2   --salt-per-kg 20 --total-hours 12   --fridge-hours 4 --warmup-hours 3 --start 09:00   --save-profile ./torino-caputo.json
```

- **Load a profile**:
```bash
cargo run -p pizza-cli -- --profile ./torino-caputo.json --temp 24 --start 08:30
```

---

## Project structure

```
pizza-workspace/
├─ Cargo.toml          # workspace definition
├─ pizza-core/         # library crate
│  ├─ src/lib.rs       # all calculations and tests
│  └─ Cargo.toml
└─ pizza-cli/          # command-line interface
   ├─ src/main.rs
   └─ Cargo.toml
```

---

## Contributing

This is an open source project born almost as a toy. Contributions are welcome, whether to improve math models, extend yeast types, add new output formats, or clean up the code.

### Guidelines
- Fork the repository and create a branch for your feature or fix.
- Add or update unit tests in `pizza-core` where applicable.
- Ensure the project builds with `cargo build --release`.
- Run tests with `cargo test`.
- Submit a pull request with a clear description of the change.

Even small improvements (docs, examples, formatting) are appreciated.

---

## License

MIT License. See [LICENSE](LICENSE) file for details.
