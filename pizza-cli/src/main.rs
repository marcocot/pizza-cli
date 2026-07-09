use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use clap::{Parser, ValueEnum};
use comfy_table::{presets::UTF8_FULL, Attribute, Cell, ContentArrangement, Table};
use pizza_core::{
    compute_ingredients, effective_hours, timeline_no_fridge, timeline_with_fridge,
    IngredientsInput, Timeline, YeastKind,
};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, process::exit};

// Defaults live here (not in clap's `default_value_t`) so we can tell an
// explicit CLI value apart from an unset one: unset stays `None`, and the
// resolution order becomes CLI > profile > default.
const DEFAULT_TEMP: f64 = 25.0;
const DEFAULT_YEAST: YeastFlag = YeastFlag::Dry;
const DEFAULT_HYDRATION: f64 = 0.75;
const DEFAULT_SALT_PER_KG: f64 = 20.0;
const DEFAULT_BALL_WEIGHT: f64 = 280.0;
const DEFAULT_BALLS: u32 = 2;
const DEFAULT_TOTAL_HOURS: f64 = 11.0;
const DEFAULT_FRIDGE_HOURS: f64 = 0.0;
const DEFAULT_WARMUP_HOURS: f64 = 3.0;
const DEFAULT_FRIDGE_FACTOR: f64 = 0.25;

const W_MIN: u16 = 200;
const W_MAX: u16 = 450;

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum YeastFlag {
    Dry,
    Fresh,
}

impl From<YeastFlag> for YeastKind {
    fn from(y: YeastFlag) -> Self {
        match y {
            YeastFlag::Dry => YeastKind::Dry,
            YeastFlag::Fresh => YeastKind::Fresh,
        }
    }
}

/// Every parameter is optional so we can distinguish "given on the CLI" from
/// "left to the profile or the default". `w` has no default: it must come from
/// either the CLI or a profile.
#[derive(Parser, Debug)]
#[command(
    name = "pizza-cli",
    about = "Calculate ingredients & timeline for Neapolitan pizza (direct dough).",
    version
)]
struct Args {
    /// Flour strength W, 200–450 [required unless supplied by a profile]
    #[arg(long, value_parser = clap::value_parser!(u16).range(W_MIN as i64..=W_MAX as i64))]
    w: Option<u16>,

    /// Ambient temperature in °C [default: 25]
    #[arg(long)]
    temp: Option<f64>,

    /// Yeast type [default: dry]
    #[arg(long, value_enum)]
    yeast: Option<YeastFlag>,

    /// Target hydration, 0.55–0.85 [default: 0.75]
    #[arg(long)]
    hydration: Option<f64>,

    /// Salt in g/kg flour [default: 20]
    #[arg(long)]
    salt_per_kg: Option<f64>,

    /// Dough ball weight in grams [default: 280]
    #[arg(long)]
    ball_weight: Option<f64>,

    /// Number of balls [default: 2]
    #[arg(long)]
    balls: Option<u32>,

    /// Total process hours, mix → bake [default: 11]
    #[arg(long)]
    total_hours: Option<f64>,

    /// Fridge time in hours, 0 = no fridge [default: 0]
    #[arg(long)]
    fridge_hours: Option<f64>,

    /// Bench rest after fridge, in hours [default: 3]
    #[arg(long)]
    warmup_hours: Option<f64>,

    /// Fridge activity vs room, 0.05–0.5 [default: 0.25]
    #[arg(long)]
    fridge_factor: Option<f64>,

    /// Start time HH:MM; defaults to now
    #[arg(long)]
    start: Option<String>,

    /// Load a profile JSON, then apply CLI overrides on top
    #[arg(long)]
    profile: Option<PathBuf>,

    /// Save the effective parameters to a profile JSON
    #[arg(long)]
    save_profile: Option<PathBuf>,
}

/// A profile holds fully-resolved values: it is what we serialize on save and
/// what we read back on load.
#[derive(Debug, Serialize, Deserialize)]
struct Profile {
    w: u16,
    temp: f64,
    yeast: YeastFlag,
    hydration: f64,
    salt_per_kg: f64,
    ball_weight: f64,
    balls: u32,
    total_hours: f64,
    fridge_hours: f64,
    warmup_hours: f64,
    fridge_factor: f64,
    start: Option<String>,
}

fn fmt_g(x: f64) -> String {
    let v = (x * 10.0).round() / 10.0;
    if (v - v.round()).abs() < 1e-9 {
        format!("{:.0} g", v)
    } else {
        format!("{:.1} g", v)
    }
}

fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("{msg}");
    exit(1);
}

fn load_profile(path: &PathBuf) -> Profile {
    let txt = fs::read_to_string(path)
        .unwrap_or_else(|e| die(format!("Failed to read profile {}: {e}", path.display())));
    serde_json::from_str(&txt)
        .unwrap_or_else(|e| die(format!("Invalid profile JSON {}: {e}", path.display())))
}

fn main() {
    let args = Args::parse();

    let profile = args.profile.as_ref().map(load_profile);
    let p = profile.as_ref();

    // Resolution order for every field: explicit CLI value, then profile, then
    // the built-in default. `w` is the exception — it has no default.
    let w = args
        .w
        .or(p.map(|p| p.w))
        .unwrap_or_else(|| die("--w is required (pass it on the CLI or via a profile)"));
    let temp = args.temp.or(p.map(|p| p.temp)).unwrap_or(DEFAULT_TEMP);
    let yeast = args.yeast.or(p.map(|p| p.yeast)).unwrap_or(DEFAULT_YEAST);
    let hydration = args
        .hydration
        .or(p.map(|p| p.hydration))
        .unwrap_or(DEFAULT_HYDRATION);
    let salt_per_kg = args
        .salt_per_kg
        .or(p.map(|p| p.salt_per_kg))
        .unwrap_or(DEFAULT_SALT_PER_KG);
    let ball_weight = args
        .ball_weight
        .or(p.map(|p| p.ball_weight))
        .unwrap_or(DEFAULT_BALL_WEIGHT);
    let balls = args.balls.or(p.map(|p| p.balls)).unwrap_or(DEFAULT_BALLS);
    let total_hours = args
        .total_hours
        .or(p.map(|p| p.total_hours))
        .unwrap_or(DEFAULT_TOTAL_HOURS);
    let fridge_hours = args
        .fridge_hours
        .or(p.map(|p| p.fridge_hours))
        .unwrap_or(DEFAULT_FRIDGE_HOURS);
    let warmup_hours = args
        .warmup_hours
        .or(p.map(|p| p.warmup_hours))
        .unwrap_or(DEFAULT_WARMUP_HOURS);
    let fridge_factor = args
        .fridge_factor
        .or(p.map(|p| p.fridge_factor))
        .unwrap_or(DEFAULT_FRIDGE_FACTOR);
    let start = args.start.clone().or_else(|| p.and_then(|p| p.start.clone()));

    // Validation. A profile can carry values that bypass clap's parsers, so we
    // re-check everything here rather than trusting the source.
    if !(W_MIN..=W_MAX).contains(&w) {
        die(format!("W must be between {W_MIN} and {W_MAX}"));
    }
    if !(0.55..=0.85).contains(&hydration) {
        die("Hydration must be between 0.55 and 0.85");
    }
    if total_hours <= 0.0 {
        die("total-hours must be > 0");
    }
    if fridge_hours < 0.0 || warmup_hours < 0.0 {
        die("fridge-hours and warmup-hours must be >= 0");
    }
    if fridge_hours > 0.0 && fridge_hours + warmup_hours >= total_hours {
        die("Sum of fridge-hours and warmup-hours must be < total-hours");
    }

    // Save once, only after everything is valid.
    if let Some(path) = &args.save_profile {
        let prof = Profile {
            w,
            temp,
            yeast,
            hydration,
            salt_per_kg,
            ball_weight,
            balls,
            total_hours,
            fridge_hours,
            warmup_hours,
            fridge_factor,
            start: start.clone(),
        };
        let json = serde_json::to_string_pretty(&prof).expect("profile serializes");
        if let Err(e) = fs::write(path, json) {
            die(format!("Failed to save profile: {e}"));
        }
        println!("Profile saved to {}", path.display());
    }

    let total_dough = balls as f64 * ball_weight;
    let eff_hours = effective_hours(total_hours, fridge_hours, fridge_factor);

    let ing = compute_ingredients(IngredientsInput {
        total_dough_g: total_dough,
        hydration,
        salt_per_kg,
        yeast: yeast.into(),
        temp_c: temp,
        w,
        effective_hours: eff_hours,
    });

    let tl: Timeline = if fridge_hours > 0.0 {
        timeline_with_fridge(total_hours, temp, fridge_hours, warmup_hours)
    } else {
        timeline_no_fridge(total_hours, temp)
    };

    // Walk the phases forward from the start time, keeping full datetimes so we
    // can flag phases that spill into the following day(s).
    let start_time = match start.as_deref() {
        Some(hhmm) => match NaiveTime::parse_from_str(hhmm, "%H:%M") {
            Ok(t) => Some(t),
            Err(_) => die(format!("Invalid --start time '{hhmm}', expected HH:MM")),
        },
        None => Some(Local::now().naive_local().time()),
    };

    let (start_date, bulk_end, fridge_end, warmup_end, proof_end) = match start_time {
        Some(st) => {
            let to_min = |h: f64| (h * 60.0).round() as i64;
            let start_dt = Local::now().date_naive().and_time(st);
            let mut dt = start_dt;

            dt += chrono::Duration::minutes(to_min(tl.bulk_h));
            let bulk_end = Some(dt);

            let fridge_end = if tl.fridge_h > 0.0 {
                dt += chrono::Duration::minutes(to_min(tl.fridge_h));
                Some(dt)
            } else {
                None
            };

            let warmup_end = if tl.warmup_h > 0.0 {
                dt += chrono::Duration::minutes(to_min(tl.warmup_h));
                Some(dt)
            } else {
                None
            };

            dt += chrono::Duration::minutes(to_min(tl.proof_h));
            let proof_end = Some(dt);

            (
                Some(start_dt.date()),
                bulk_end,
                fridge_end,
                warmup_end,
                proof_end,
            )
        }
        None => (None, None, None, None, None),
    };

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Ingredient").add_attribute(Attribute::Bold),
            Cell::new("Amount").add_attribute(Attribute::Bold),
            Cell::new("Notes").add_attribute(Attribute::Bold),
        ]);

    table.add_row(vec![
        Cell::new("Balls"),
        Cell::new(format!("{} × {:.0} g", balls, ball_weight)),
        Cell::new(""),
    ]);
    table.add_row(vec![
        Cell::new("Flour"),
        Cell::new(fmt_g(ing.flour_g)),
        Cell::new(format!("W={} | H={:.0}%", w, hydration * 100.0)),
    ]);
    table.add_row(vec![
        Cell::new("Water"),
        Cell::new(fmt_g(ing.water_g)),
        Cell::new(""),
    ]);
    table.add_row(vec![
        Cell::new("Salt"),
        Cell::new(fmt_g(ing.salt_g)),
        Cell::new(format!("{:.1} g/kg", salt_per_kg)),
    ]);

    match yeast {
        YeastFlag::Dry => table.add_row(vec![
            Cell::new("Dry yeast"),
            Cell::new(fmt_g(ing.yeast_g)),
            Cell::new("~% of flour (estimate)"),
        ]),
        YeastFlag::Fresh => table.add_row(vec![
            Cell::new("Fresh yeast"),
            Cell::new(fmt_g(ing.yeast_g)),
            Cell::new("~3× dry yeast"),
        ]),
    };

    println!("\n=== Ingredients summary ===");
    println!("{table}");

    println!("\n=== Timeline ===");
    println!(
        "- Bulk rise (whole dough): {:.1} h{}",
        tl.bulk_h,
        fmt_end(start_date, bulk_end)
    );

    if tl.fridge_h > 0.0 {
        println!(
            "- Fridge (covered):        {:.1} h{}",
            tl.fridge_h,
            fmt_end(start_date, fridge_end)
        );
        println!(
            "- Warmup (bench rest):     {:.1} h{}",
            tl.warmup_h,
            fmt_end(start_date, warmup_end)
        );
    }

    println!(
        "- Final proof (balls):     {:.1} h{}",
        tl.proof_h,
        fmt_end(start_date, proof_end)
    );

    println!(
        "- Total:                   {:.1} h",
        tl.bulk_h + tl.fridge_h + tl.warmup_h + tl.proof_h
    );

    println!("\nNotes:");
    println!("• Yeast amounts are heuristic (Q10≈2/10°C; mild W effect). Fridge counted at configurable factor.");
    println!("• If dough rises too fast in warm conditions (>27°C), shorten bulk or reduce yeast slightly.");
}

/// Format a phase end time, appending `(+Nd)` when it lands on a later day than
/// the start so long fridge timelines aren't silently misread.
fn fmt_end(start_date: Option<NaiveDate>, end: Option<NaiveDateTime>) -> String {
    match (start_date, end) {
        (Some(start), Some(dt)) => {
            let days = (dt.date() - start).num_days();
            if days > 0 {
                format!(" → ~end at {:02}:{:02} (+{}d)", dt.hour(), dt.minute(), days)
            } else {
                format!(" → ~end at {:02}:{:02}", dt.hour(), dt.minute())
            }
        }
        _ => String::new(),
    }
}
