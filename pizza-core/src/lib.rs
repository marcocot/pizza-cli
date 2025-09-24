use serde::{Deserialize, Serialize};

/// Yeast kind supported by the core.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum YeastKind {
    Dry,
    Fresh,
}

/// Input for ingredient computation.
#[derive(Copy, Clone, Debug)]
pub struct IngredientsInput {
    /// Total dough weight in grams (sum of all balls).
    pub total_dough_g: f64,
    /// Target hydration as fraction (e.g., 0.75 for 75%).
    pub hydration: f64,
    /// Salt per kg flour in g/kg (e.g., 20.0).
    pub salt_per_kg: f64,
    /// Yeast type.
    pub yeast: YeastKind,
    /// Ambient temperature in °C (for yeast estimates).
    pub temp_c: f64,
    /// Flour strength W (approx for mild effect).
    pub w: u16,
    /// Effective fermentation hours (counts fridge slower than room).
    pub effective_hours: f64,
}

/// Output ingredients (in grams).
#[derive(Copy, Clone, Debug)]
pub struct Ingredients {
    pub flour_g: f64,
    pub water_g: f64,
    pub salt_g: f64,
    /// For baker’s yeast (dry/fresh).
    pub yeast_g: f64,
    /// For sourdough only: total starter (flour+water) at 100% hydration.
    pub starter_total_g: f64,
}

#[inline]
fn clamp<T: PartialOrd>(v: T, lo: T, hi: T) -> T {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

/// Dry yeast percent of flour (fraction, e.g., 0.0035 = 0.35%)
/// Baseline: 0.35% at 25°C, W=260, 12h.
/// Q10 ≈ 2 per 10°C, mild W effect, inverse with time.
pub fn estimate_yeast_percent_dry(temp_c: f64, w: u16, effective_hours: f64) -> f64 {
    let base = 0.0035;
    let f_temp = 2f64.powf((25.0 - temp_c) / 10.0);
    let f_w = (w as f64 / 260.0).powf(0.2);
    let f_time = 12.0 / effective_hours;
    clamp(base * f_temp * f_w * f_time, 0.0005, 0.015) // 0.05%..1.5%
}

/// Effective hours model:
/// Counts room hours fully and fridge hours at `fridge_factor` speed (default 0.25).
pub fn effective_hours(total_hours: f64, fridge_hours: f64, fridge_factor: f64) -> f64 {
    let fridge_hours = fridge_hours.max(0.0).min(total_hours.max(0.0));
    let rf = clamp(fridge_factor, 0.05, 0.5);
    (total_hours - fridge_hours) + fridge_hours * rf
}

/// Compute ingredients for given input.
/// - Dry/Fresh: dough = flour + water + salt + yeast
/// - Sourdough: dough = flour + water + salt, where part of flour+water comes from starter (100%)
pub fn compute_ingredients(input: IngredientsInput) -> Ingredients {
    let salt_pct = input.salt_per_kg / 1000.0;
    let h = input.hydration;

    match input.yeast {
        YeastKind::Dry | YeastKind::Fresh => {
            let dry_pct = estimate_yeast_percent_dry(input.temp_c, input.w, input.effective_hours);
            let yeast_pct = match input.yeast {
                YeastKind::Dry => dry_pct,
                YeastKind::Fresh => dry_pct * 3.0,
            };

            let flour = input.total_dough_g / (1.0 + h + salt_pct + yeast_pct);
            let water = flour * h;
            let salt = flour * salt_pct;
            let yeast = flour * yeast_pct;

            Ingredients {
                flour_g: flour,
                water_g: water,
                salt_g: salt,
                yeast_g: yeast,
                starter_total_g: 0.0,
            }
        }
    }
}

/// Timeline (hours) for dough workflow.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Timeline {
    pub bulk_h: f64,
    pub fridge_h: f64,
    pub warmup_h: f64,
    pub proof_h: f64,
}

fn temp_adjust_ratio(temp_c: f64, base: f64, step: f64, min: f64, max: f64) -> f64 {
    if temp_c > 25.0 {
        (base - ((temp_c - 25.0) * step)).max(min)
    } else if temp_c < 25.0 {
        (base + ((25.0 - temp_c) * step)).min(max)
    } else {
        base
    }
}

/// No-fridge timeline: split total into bulk/proof ~55/45 with temp adjustment.
pub fn timeline_no_fridge(total_hours: f64, temp_c: f64) -> Timeline {
    let mut bulk = total_hours * 0.55;
    let mut proof = total_hours - bulk;

    // shift up to ~1h from bulk→proof when hot, or the opposite when cold
    if temp_c > 25.0 {
        let delta = ((temp_c - 25.0) * 0.05).clamp(0.0, 1.0);
        let adjust = delta.min(bulk * 0.2);
        bulk -= adjust;
        proof += adjust;
    } else if temp_c < 25.0 {
        let delta = ((25.0 - temp_c) * 0.05).clamp(0.0, 1.0);
        let adjust = delta.min(proof * 0.2);
        bulk += adjust;
        proof -= adjust;
    }

    Timeline {
        bulk_h: bulk,
        fridge_h: 0.0,
        warmup_h: 0.0,
        proof_h: proof,
    }
}

/// Fridge timeline: total = bulk + fridge + warmup + proof.
/// We split the remaining (after fridge+warmup) using a temp-adjusted ratio.
pub fn timeline_with_fridge(
    total_hours: f64,
    temp_c: f64,
    fridge_hours: f64,
    warmup_hours: f64,
) -> Timeline {
    let remaining = (total_hours - fridge_hours - warmup_hours).max(0.0);
    // Base bulk ratio of remaining is 35%, adjusted by temperature
    let bulk_ratio = temp_adjust_ratio(temp_c, 0.35, 0.01, 0.20, 0.60);
    let bulk = remaining * bulk_ratio;
    let proof = remaining - bulk;

    Timeline {
        bulk_h: bulk,
        fridge_h: fridge_hours.max(0.0),
        warmup_h: warmup_hours.max(0.0),
        proof_h: proof,
    }
}

/* ===========================
Unit tests
=========================== */

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_effective_hours_limits() {
        let e = effective_hours(12.0, 4.0, 0.25); // = 12 - 4 + 4*0.25 = 9.0
        assert!((e - 9.0).abs() < 1e-9);

        // fridge factor is clamped to >= 0.05, so 0.01 -> 0.05
        let e2 = effective_hours(12.0, 4.0, 0.01); // = 12 - 4 + 4*0.05 = 8.2
        assert!(
            e2 < e,
            "with a slower fridge factor, effective hours should be lower"
        );
    }

    #[test]
    fn test_yeast_percent_bounds() {
        let p_lo = estimate_yeast_percent_dry(35.0, 260, 24.0);
        let p_hi = estimate_yeast_percent_dry(10.0, 450, 6.0);
        assert!(p_lo >= 0.0005 && p_lo <= 0.015);
        assert!(p_hi >= 0.0005 && p_hi <= 0.015);
    }

    #[test]
    fn test_ingredients_sum_dry() {
        let input = IngredientsInput {
            total_dough_g: 560.0,
            hydration: 0.75,
            salt_per_kg: 20.0,
            yeast: YeastKind::Dry,
            temp_c: 25.0,
            w: 270,
            effective_hours: 11.0,
        };
        let out = compute_ingredients(input);
        let sum = out.flour_g + out.water_g + out.salt_g + out.yeast_g;
        assert_relative_eq!(sum, 560.0, epsilon = 0.2);
    }

    #[test]
    fn test_timeline_no_fridge_sums() {
        let t = timeline_no_fridge(11.0, 25.0);
        assert_relative_eq!(t.bulk_h + t.proof_h, 11.0, epsilon = 1e-9);
        assert_relative_eq!(t.fridge_h, 0.0, epsilon = 1e-9);
    }

    #[test]
    fn test_timeline_with_fridge_sums() {
        let t = timeline_with_fridge(12.0, 25.0, 4.0, 3.0);
        assert_relative_eq!(
            t.bulk_h + t.proof_h + t.fridge_h + t.warmup_h,
            12.0,
            epsilon = 1e-9
        );
    }
}
