use clap::{ArgGroup, Parser, ValueEnum};
use chrono::{Local, NaiveTime, Timelike};
use comfy_table::{presets::UTF8_FULL, Attribute, Cell, ContentArrangement, Table};
use pizza_core::{
    compute_ingredients, effective_hours, timeline_no_fridge, timeline_with_fridge, IngredientsInput,
    Timeline, YeastKind,
};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

/// Yeast CLI enum mirrors pizza-core (derive for Clap).
#[derive(Copy, Clone, Debug, ValueEnum, Serialize, Deserialize)]
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

#[derive(Parser, Debug, Serialize, Deserialize)]
#[command(
    name="pizza-cli",
    about="Calculate ingredients & timeline for Neapolitan pizza (direct dough).",
    version
)]
#[command(group(
    ArgGroup::new("time_group")
        .args(["total_hours"])
        .required(false)
))]
struct Args {
    /// Flour strength W (e.g., 260–300)
    #[arg(long, value_parser = clap::value_parser!(u16).range(200..=450))]
    w: u16,

    /// Ambient temperature in °C
    #[arg(long, default_value_t = 25.0)]
    temp: f64,

    /// Yeast type
    #[arg(long, value_enum, default_value_t = YeastFlag::Dry)]
    yeast: YeastFlag,

    /// Target hydration (0.55..0.85)
    #[arg(long, default_value_t = 0.75)]
    hydration: f64,

    /// Salt in g/kg flour
    #[arg(long, default_value_t = 20.0)]
    salt_per_kg: f64,

    /// Dough ball weight in grams
    #[arg(long, default_value_t = 280.0)]
    ball_weight: f64,

    /// Number of balls
    #[arg(long, default_value_t = 2)]
    balls: u32,

    /// Total process hours (mix → bake)
    #[arg(long, default_value_t = 11.0)]
    total_hours: f64,

    /// Fridge time in hours (0 = no fridge mode)
    #[arg(long, default_value_t = 0.0)]
    fridge_hours: f64,

    /// Warmup time after fridge (bench rest) in hours
    #[arg(long, default_value_t = 3.0)]
    warmup_hours: f64,

    /// Fridge factor (activity speed vs room), default 0.25
    #[arg(long, default_value_t = 0.25)]
    fridge_factor: f64,

    /// Start time HH:MM (optional); defaults to now
    #[arg(long)]
    start: Option<String>,

    /// Load a profile JSON before applying CLI overrides
    #[arg(long)]
    profile: Option<PathBuf>,

    /// Save the current effective parameters to a profile JSON
    #[arg(long)]
    save_profile: Option<PathBuf>,
}

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

impl From<&Args> for Profile {
    fn from(a: &Args) -> Self {
        Profile {
            w: a.w,
            temp: a.temp,
            yeast: a.yeast,
            hydration: a.hydration,
            salt_per_kg: a.salt_per_kg,
            ball_weight: a.ball_weight,
            balls: a.balls,
            total_hours: a.total_hours,
            fridge_hours: a.fridge_hours,
            warmup_hours: a.warmup_hours,
            fridge_factor: a.fridge_factor,
            start: a.start.clone(),
        }
    }
}

fn fmt_g(x: f64) -> String {
    let v = (x * 10.0).round() / 10.0;
    if (v - v.round()).abs() < 1e-9 {
        format!("{:.0} g", v)
    } else {
        format!("{:.1} g", v)
    }
}

fn main() {
    let mut args = Args::parse();

    // Load profile if present, then apply CLI overrides (CLI wins).
    if let Some(path) = &args.profile {
        let Ok(txt) = fs::read_to_string(path) else {
            eprintln!("Failed to read profile: {}", path.display());
            std::process::exit(1);
        };
        let Ok(p): Result<Profile, _> = serde_json::from_str(&txt) else {
            eprintln!("Invalid profile JSON: {}", path.display());
            std::process::exit(1);
        };

        // Defaults snapshot to detect "unset" fields
        let def = Args::parse_from(["pizza-cli"]);

        macro_rules! take {
            ($field:ident) => {
                if args.$field == def.$field { p.$field } else { args.$field }
            };
        }

        args.w = take!(w);
        args.temp = take!(temp);
        args.yeast = if matches!(args.yeast, YeastFlag::Dry) && !matches!(p.yeast, YeastFlag::Dry) {
            p.yeast
        } else {
            args.yeast
        };
        args.hydration = take!(hydration);
        args.salt_per_kg = take!(salt_per_kg);
        args.ball_weight = take!(ball_weight);
        args.balls = take!(balls);
        args.total_hours = take!(total_hours);
        args.fridge_hours = take!(fridge_hours);
        args.warmup_hours = take!(warmup_hours);
        args.fridge_factor = take!(fridge_factor);
        if args.start.is_none() {
            args.start = p.start;
        }
    }

    // Save profile if requested (using the effective arguments).
    if let Some(path) = &args.save_profile {
        let prof = Profile::from(&args);
        if let Err(e) = fs::write(path, serde_json::to_string_pretty(&prof).unwrap()) {
            eprintln!("Failed to save profile: {e}");
            std::process::exit(1);
        } else {
            println!("Profile saved to {}", path.display());
        }
    }

    // Validations
    if !(0.55..=0.85).contains(&args.hydration) {
        eprintln!("Hydration must be between 0.55 and 0.85");
        std::process::exit(1);
    }
    if args.total_hours <= 0.0 {
        eprintln!("total-hours must be > 0");
        std::process::exit(1);
    }
    if args.fridge_hours < 0.0 || args.warmup_hours < 0.0 {
        eprintln!("fridge-hours and warmup-hours must be >= 0");
        std::process::exit(1);
    }
    if args.fridge_hours > 0.0 && args.fridge_hours + args.warmup_hours >= args.total_hours {
        eprintln!("Sum of fridge-hours and warmup-hours must be < total-hours");
        std::process::exit(1);
    }

    // Totals
    let balls = args.balls as f64;
    let total_dough = balls * args.ball_weight;

    // Effective hours for yeast model
    let eff_hours = effective_hours(args.total_hours, args.fridge_hours, args.fridge_factor);

    // Ingredients
    let ing = compute_ingredients(IngredientsInput {
        total_dough_g: total_dough,
        hydration: args.hydration,
        salt_per_kg: args.salt_per_kg,
        yeast: args.yeast.into(),
        temp_c: args.temp,
        w: args.w,
        effective_hours: eff_hours,
    });

    // Timeline (with/without fridge)
    let tl: Timeline = if args.fridge_hours > 0.0 {
        timeline_with_fridge(args.total_hours, args.temp, args.fridge_hours, args.warmup_hours)
    } else {
        timeline_no_fridge(args.total_hours, args.temp)
    };

    // Start time and phase ends
    let start_time = if let Some(hhmm) = args.start.as_ref() {
        NaiveTime::parse_from_str(hhmm, "%H:%M").ok()
    } else {
        Some(Local::now().naive_local().time())
    };

    let (t_bulk_end, t_fridge_end, t_warmup_end, t_proof_end) = if let Some(st) = start_time {
        let to_min = |h: f64| (h * 60.0).round() as i64;
        let mut dt = Local::now().date_naive().and_time(st);

        let bulk_end = dt + chrono::Duration::minutes(to_min(tl.bulk_h));
        dt = bulk_end;

        let fridge_end = if tl.fridge_h > 0.0 {
            let e = dt + chrono::Duration::minutes(to_min(tl.fridge_h));
            dt = e;
            Some(e)
        } else {
            None
        };

        let warmup_end = if tl.warmup_h > 0.0 {
            let e = dt + chrono::Duration::minutes(to_min(tl.warmup_h));
            dt = e;
            Some(e)
        } else {
            None
        };

        let proof_end = dt + chrono::Duration::minutes(to_min(tl.proof_h));
        (
            Some(bulk_end.time()),
            fridge_end.map(|x| x.time()),
            warmup_end.map(|x| x.time()),
            Some(proof_end.time()),
        )
    } else {
        (None, None, None, None)
    };

    // Ingredients table
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
        Cell::new(format!("{} × {:.0} g", args.balls, args.ball_weight)),
        Cell::new(""),
    ]);
    table.add_row(vec![
        Cell::new("Flour"),
        Cell::new(fmt_g(ing.flour_g)),
        Cell::new(format!("W={} | H={:.0}%", args.w, args.hydration * 100.0)),
    ]);
    table.add_row(vec![Cell::new("Water"), Cell::new(fmt_g(ing.water_g)), Cell::new("")]);
    table.add_row(vec![
        Cell::new("Salt"),
        Cell::new(fmt_g(ing.salt_g)),
        Cell::new(format!("{:.1} g/kg", args.salt_per_kg)),
    ]);

    match args.yeast {
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
    println!("{}", table);

    // Timeline
    println!("\n=== Timeline ===");
    println!(
        "- Bulk rise (whole dough): {:.1} h{}",
        tl.bulk_h,
        match t_bulk_end {
            Some(t) => format!(" → ~end at {:02}:{:02}", t.hour(), t.minute()),
            None => "".to_string(),
        }
    );

    if tl.fridge_h > 0.0 {
        println!(
            "- Fridge (covered):        {:.1} h{}",
            tl.fridge_h,
            match t_fridge_end {
                Some(t) => format!(" → ~end at {:02}:{:02}", t.hour(), t.minute()),
                None => "".to_string(),
            }
        );
        println!(
            "- Warmup (bench rest):     {:.1} h{}",
            tl.warmup_h,
            match t_warmup_end {
                Some(t) => format!(" → ~end at {:02}:{:02}", t.hour(), t.minute()),
                None => "".to_string(),
            }
        );
    }

    println!(
        "- Final proof (balls):     {:.1} h{}",
        tl.proof_h,
        match t_proof_end {
            Some(t) => format!(" → ~end at {:02}:{:02}", t.hour(), t.minute()),
            None => "".to_string(),
        }
    );

    println!(
        "- Total:                   {:.1} h",
        tl.bulk_h + tl.fridge_h + tl.warmup_h + tl.proof_h
    );

    println!("\nNotes:");
    println!("• Yeast amounts are heuristic (Q10≈2/10°C; mild W effect). Fridge counted at configurable factor.");
    println!("• If dough rises too fast in warm conditions (>27°C), shorten bulk or reduce yeast slightly.");

    // Save profile at the end if requested (again, to reflect any defaults resolved)
    if let Some(path) = &args.save_profile {
        let prof = Profile::from(&args);
        let _ = fs::write(path, serde_json::to_string_pretty(&prof).unwrap());
    }
}
