use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

pub const READING_GRACE: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Auto,
    Ship,
    Salvage,
    Roc,
    Fps,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Resource {
    pub name: String,
    pub rs: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Catalog {
    pub version: String,
    pub max_rocks: u32,
    pub resources: Vec<Resource>,
    #[serde(default = "default_panel_rs")]
    pub salvage_panel_rs: u32,
    #[serde(default = "default_max_panels")]
    pub max_panels: u32,
}
fn default_panel_rs() -> u32 {
    2000
}
fn default_max_panels() -> u32 {
    100
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Kind {
    Rock,
    Deposit,
    Panel,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Match {
    pub name: String,
    pub count: u32,
    pub base: u32,
    pub kind: Kind,
}
impl Match {
    pub fn quantity(&self) -> String {
        let unit = match (self.kind, self.count == 1) {
            (Kind::Rock, true) => "rock",
            (Kind::Rock, false) => "rocks",
            (Kind::Deposit, true) => "deposit",
            (Kind::Deposit, false) => "deposits",
            (Kind::Panel, true) => "panel",
            (Kind::Panel, false) => "panels",
        };
        format!("{} {unit}", self.count)
    }
    pub fn label(&self) -> String {
        format!("{} · {}", self.name, self.quantity())
    }
}

impl Catalog {
    pub fn from_json(json: &str) -> Result<Self, String> {
        let c: Self = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if !(1..=100).contains(&c.max_rocks)
            || !(1..=100).contains(&c.max_panels)
            || !(1000..=100000).contains(&c.salvage_panel_rs)
            || c.resources.is_empty()
            || c.resources
                .iter()
                .any(|r| r.name.trim().is_empty() || !(1000..=100000).contains(&r.rs))
        {
            return Err("Invalid signature catalog".into());
        }
        Ok(c)
    }
    pub fn builtin() -> Self {
        Self::from_json(include_str!("../data/signatures.json")).expect("bundled signatures")
    }
    pub fn resolve(&self, value: u32, mode: Mode) -> Vec<Match> {
        let mut matches = Vec::new();
        if value == 0 {
            return matches;
        }
        if matches!(mode, Mode::Auto | Mode::Ship) {
            matches.extend(
                self.resources
                    .iter()
                    .filter(|r| value.is_multiple_of(r.rs) && value / r.rs <= self.max_rocks)
                    .map(|r| Match {
                        name: r.name.clone(),
                        count: value / r.rs,
                        base: r.rs,
                        kind: Kind::Rock,
                    }),
            );
        }
        if matches!(mode, Mode::Auto | Mode::Salvage)
            && value.is_multiple_of(self.salvage_panel_rs)
            && value / self.salvage_panel_rs <= self.max_panels
        {
            matches.push(Match {
                name: "Salvage".into(),
                count: value / self.salvage_panel_rs,
                base: self.salvage_panel_rs,
                kind: Kind::Panel,
            });
        }
        if matches!(mode, Mode::Roc | Mode::Fps) {
            let (name, base) = if mode == Mode::Roc {
                ("ROC deposit", 4000)
            } else {
                ("FPS deposit", 3000)
            };
            if value.is_multiple_of(base) && value / base <= self.max_rocks {
                matches.push(Match {
                    name: name.into(),
                    count: value / base,
                    base,
                    kind: Kind::Deposit,
                });
            }
        }
        matches
    }
    pub fn hud_text(&self, value: u32, mode: Mode) -> (String, String) {
        let matches = self.resolve(value, mode);
        match matches.as_slice() {
            [found] => (found.name.clone(), found.quantity()),
            [] => (format!("RS {value}"), "Unknown".into()),
            many => (
                many.iter()
                    .map(|m| format!("{} ×{}", m.name, m.count))
                    .collect::<Vec<_>>()
                    .join(" / "),
                "?".into(),
            ),
        }
    }
}

/// Parse only a complete exact RS value. Never turn a distance, abbreviation,
/// several lines, or a guessed OCR letter into a plausible mineral.
pub fn parse_signature(text: &str) -> Option<u32> {
    let trimmed = text.trim();
    let body = if trimmed
        .get(..2)
        .is_some_and(|s| s.eq_ignore_ascii_case("rs"))
    {
        trimmed[2..]
            .trim_start()
            .trim_start_matches([':', '='])
            .trim_start()
    } else {
        trimmed
    };
    if body.is_empty() {
        return None;
    }
    let normalized = body.replace(", ", ",").replace(". ", ".");
    let groups: Vec<_> = normalized
        .split([',', '.', ' ', '\u{a0}', '\u{202f}'])
        .collect();
    if groups.len() > 1
        && (groups[0].is_empty() || groups[0].len() > 3 || groups[1..].iter().any(|s| s.len() != 3))
    {
        return None;
    }
    let digits = groups.concat();
    if !(4..=6).contains(&digits.len()) || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let value = digits.parse().ok()?;
    (1000..=999999).contains(&value).then_some(value)
}

#[derive(Default)]
pub struct StableReading {
    candidate: Option<u32>,
    repeats: u8,
    confirmed: Option<(u32, Instant)>,
    candidate_at: Option<Instant>,
}
impl StableReading {
    pub fn push(&mut self, value: Option<u32>, now: Instant) -> Option<u32> {
        if self
            .candidate_at
            .is_some_and(|at| now.duration_since(at) > Duration::from_secs(2))
        {
            self.candidate = None;
            self.repeats = 0;
            self.candidate_at = None;
        }
        if self
            .confirmed
            .is_some_and(|(_, at)| now.duration_since(at) >= READING_GRACE)
        {
            self.confirmed = None;
        }
        if let Some(value) = value {
            if self
                .confirmed
                .is_some_and(|(previous, _)| previous == value)
            {
                self.confirmed = Some((value, now));
                return Some(value);
            }
            // A different readable signature invalidates the old target immediately.
            self.confirmed = None;
        }
        // An unreadable frame is not contradictory evidence. Two equal positive
        // reads within two seconds can confirm even with a brief miss in between.
        if value.is_none() {
            return self.confirmed.map(|(v, _)| v);
        }
        self.candidate_at = Some(now);
        if self.candidate != value {
            self.candidate = value;
            self.repeats = u8::from(value.is_some());
        } else {
            self.repeats = self.repeats.saturating_add(1);
            if self.repeats >= 2 {
                self.confirmed = value.map(|v| (v, now));
            }
        }
        self.confirmed.map(|(v, _)| v)
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_exact_readings() {
        for s in [
            "10800",
            "10,800",
            "10.800",
            "10 800",
            "10\u{202f}800",
            "RS: 10,800",
            " 10800 ",
        ] {
            assert_eq!(parse_signature(s), Some(10800), "{s}");
        }
    }
    #[test]
    fn rejects_distances_rounded_values_and_noise() {
        for s in [
            "10.8K",
            "10800 m",
            "10,80",
            "10,8OO",
            "",
            "5.0km",
            "10800\n6400",
            "10 800 6400",
            "0",
            "9999999",
            "21.6K",
            "10,800/20,000",
        ] {
            assert_eq!(parse_signature(s), None, "{s}");
        }
    }
    #[test]
    fn screenshot_bexalite_and_unknown() {
        let c = Catalog::builtin();
        let m = c.resolve(10800, Mode::Ship);
        assert_eq!(
            m,
            vec![Match {
                name: "Bexalite".into(),
                count: 3,
                base: 3600,
                kind: Kind::Rock
            }]
        );
        assert!(c.resolve(10801, Mode::Ship).is_empty());
        assert!(c.resolve(0, Mode::Ship).is_empty());
        assert!(c.resolve(3600 * 101, Mode::Ship).is_empty());
    }
    #[test]
    fn preserves_collisions() {
        let c = Catalog::builtin();
        let m = c.resolve(38400, Mode::Ship);
        assert!(m.iter().any(|m| m.name == "Savrilium" && m.count == 12));
        assert!(m.iter().any(|m| m.name == "Aslarite" && m.count == 10));
    }
    #[test]
    fn ground_mode_never_invents_gem() {
        let c = Catalog::builtin();
        assert_eq!(c.resolve(8000, Mode::Roc)[0].name, "ROC deposit");
        assert_eq!(c.resolve(9000, Mode::Fps)[0].count, 3);
        assert!(c.resolve(10800, Mode::Roc).is_empty());
    }
    #[test]
    fn auto_detects_salvage_panels_with_the_right_units() {
        let c = Catalog::builtin();
        for count in [1, 2, 4, 13, 100] {
            let found = c.resolve(count * 2000, Mode::Salvage);
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].kind, Kind::Panel);
            assert_eq!(found[0].count, count);
        }
        assert_eq!(
            c.hud_text(2000, Mode::Auto),
            ("Salvage".into(), "1 panel".into())
        );
        assert_eq!(
            c.hud_text(8000, Mode::Auto),
            ("Salvage".into(), "4 panels".into())
        );
        assert_eq!(
            c.hud_text(10800, Mode::Auto),
            ("Bexalite".into(), "3 rocks".into())
        );
        assert!(c.resolve(2001, Mode::Auto).is_empty());
        assert!(c.resolve(0, Mode::Salvage).is_empty());
        assert!(c.resolve(202000, Mode::Salvage).is_empty());
    }
    #[test]
    fn salvage_mining_collision_is_visible_and_activity_filters_remain_explicit() {
        let c = Catalog::builtin();
        let matches = c.resolve(18000, Mode::Auto);
        assert!(matches
            .iter()
            .any(|m| m.kind == Kind::Rock && m.name == "Bexalite" && m.count == 5));
        assert!(matches
            .iter()
            .any(|m| m.kind == Kind::Panel && m.count == 9));
        let (label, marker) = c.hud_text(18000, Mode::Auto);
        assert!(label.contains("Bexalite ×5") && label.contains("Salvage ×9"));
        assert_eq!(marker, "?");
        assert_eq!(
            c.resolve(18000, Mode::Salvage)[0].label(),
            "Salvage · 9 panels"
        );
        assert!(c.resolve(8000, Mode::Ship).is_empty());
        assert_eq!(
            c.resolve(8000, Mode::Roc)[0].label(),
            "ROC deposit · 2 deposits"
        );
        assert!(c.resolve(10800, Mode::Salvage).is_empty());
    }
    #[test]
    fn older_custom_catalogs_receive_salvage_defaults() {
        let c = Catalog::from_json(
            r#"{"version":"old","max_rocks":12,"resources":[{"name":"Bexalite","rs":3600}]}"#,
        )
        .unwrap();
        assert_eq!(c.resolve(8000, Mode::Auto)[0].label(), "Salvage · 4 panels");
        for field in ["salvage_panel_rs", "max_panels"] {
            let mut json: serde_json::Value =
                serde_json::from_str(include_str!("../data/signatures.json")).unwrap();
            json[field] = 0.into();
            assert!(Catalog::from_json(&json.to_string()).is_err());
        }
    }
    #[test]
    fn requires_confirmation_and_replaces_a_changed_target() {
        let mut s = StableReading::default();
        let now = Instant::now();
        assert_eq!(s.push(Some(10800), now), None);
        assert_eq!(s.push(Some(10800), now), Some(10800));
        assert_eq!(s.push(Some(6400), now), None);
        assert_eq!(s.push(None, now), None);
        assert_eq!(s.push(Some(6400), now), Some(6400));
        s.reset();
        assert_eq!(s.push(Some(6400), now), None);
    }
    #[test]
    fn brief_misses_do_not_starve_confirmation_but_old_single_reads_expire() {
        let mut s = StableReading::default();
        let now = Instant::now();
        assert_eq!(s.push(Some(10800), now), None);
        assert_eq!(s.push(None, now + Duration::from_millis(250)), None);
        assert_eq!(
            s.push(Some(10800), now + Duration::from_millis(500)),
            Some(10800)
        );
        s.reset();
        s.push(Some(10800), now);
        assert_eq!(s.push(Some(10800), now + Duration::from_secs(3)), None);
        assert_eq!(s.push(Some(6400), now + Duration::from_millis(3250)), None);
        assert_eq!(s.push(Some(10800), now + Duration::from_millis(3500)), None);
    }
    #[test]
    fn tolerates_brief_ocr_misses_but_expires_a_lost_target() {
        let mut s = StableReading::default();
        let now = Instant::now();
        s.push(Some(10800), now);
        s.push(Some(10800), now);
        assert_eq!(s.push(None, now + Duration::from_secs(1)), Some(10800));
        assert_eq!(s.push(None, now + Duration::from_secs(4)), Some(10800));
        assert_eq!(s.push(None, now + READING_GRACE), None);
        assert_eq!(s.push(Some(10800), now + Duration::from_secs(6)), None);
        assert_eq!(
            s.push(Some(10800), now + Duration::from_secs(7)),
            Some(10800)
        );
    }
    #[test]
    fn visible_signature_does_not_expire_and_recovers_without_blinking() {
        let mut s = StableReading::default();
        let now = Instant::now();
        s.push(Some(10800), now);
        s.push(Some(10800), now);
        for seconds in 1..600 {
            let value = if seconds % 3 == 0 { Some(10800) } else { None };
            assert_eq!(
                s.push(value, now + Duration::from_secs(seconds)),
                Some(10800)
            );
        }
    }
    #[test]
    fn rejects_invalid_catalog() {
        assert!(Catalog::from_json(
            r#"{"version":"x","max_rocks":12,"resources":[{"name":"bad","rs":0}]}"#
        )
        .is_err());
    }
}
