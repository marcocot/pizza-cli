# pizza-cli

A little command-line tool that works out how much flour, water, salt and yeast
go into a Neapolitan-style dough, and roughly when to start each step so the
balls are ready when you want to bake.

It started as a way to stop me doing the same arithmetic on a napkin every
Saturday. The numbers are heuristics, not gospel — treat them as a starting
point and trust your dough over the table.

The repo is a small Cargo workspace: `pizza-core` holds the math (and its
tests), `pizza-cli` is the thing you actually run.

## Building

You need a recent Rust toolchain — the crates are on the 2024 edition, so
**1.85 or newer**. Get it from [rustup](https://rustup.rs) if you don't have it.

```bash
git clone https://github.com/marcocot/pizza-cli.git
cd pizza-cli
cargo build --release
```

The binary lands in `target/release/pizza-cli`.

## Running

You have to give it the flour strength (`--w`); everything else has a sensible
default. A plain same-day dough:

```bash
cargo run -p pizza-cli -- --w 270 --hydration 0.75 --balls 2 --total-hours 11 --start 09:00
```

Cold-fermented in the fridge, fresh yeast:

```bash
cargo run -p pizza-cli -- --w 270 --yeast fresh --hydration 0.70 \
  --balls 4 --ball-weight 260 --salt-per-kg 22 \
  --total-hours 24 --fridge-hours 16 --warmup-hours 3 --start 18:00
```

Timeline steps that spill past midnight are marked with `(+1d)`, so a 24-hour
fridge plan doesn't quietly look like it finishes the same evening.

`--help` lists every flag with its default. The defaults are dry yeast, 75%
hydration, 20 g/kg salt, 280 g balls, 2 balls, 11 total hours, no fridge.

### Profiles

If you keep making the same dough, save the parameters and reload them later:

```bash
# save
cargo run -p pizza-cli -- --w 270 --hydration 0.75 --total-hours 12 \
  --fridge-hours 4 --start 09:00 --save-profile torino.json

# reload — the profile even carries --w, so you don't repeat it
cargo run -p pizza-cli -- --profile torino.json --temp 24 --start 08:30
```

Anything you pass on the command line overrides the profile; the profile
overrides the built-in defaults.

## How the numbers are worked out

Ingredients are baker's math: total dough weight is `balls × ball-weight`, and
flour is solved so `flour + water + salt + yeast` adds back up to that total.
Water is `flour × hydration`, salt is the g/kg you asked for, yeast is an
estimated percentage of the flour.

The yeast estimate is the interesting bit. It starts from ~0.35% dry yeast at
25 °C, W=260, over 12 hours, then scales it:

- temperature: Q10 ≈ 2, so every 10 °C roughly halves or doubles the amount;
- flour strength: a mild pull (W to the power 0.2) — stronger flour, a touch
  more yeast;
- time: inversely, longer ferment needs less yeast.

The result is clamped to a sane 0.05%–1.5%. Fresh yeast is just treated as ~3×
dry. Fridge time counts as slower fermentation via a `--fridge-factor` (default
0.25, i.e. four hours in the fridge ≈ one at room temperature), which feeds the
"effective hours" the yeast math actually sees.

The timeline splits the total into bulk and proof (and, with a fridge, the cold
stretch plus a bench warm-up in between), nudging the bulk/proof balance a
little with temperature.

## Contributing

Patches welcome — better models, more yeast types, nicer output, whatever. Keep
`cargo test` green and run `cargo clippy` before opening a PR.

## License

MIT — see [LICENSE](LICENSE).
