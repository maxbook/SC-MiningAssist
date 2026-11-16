use crate::core::{Catalog, Kind, Mode};
use serde::Deserialize;
use std::{collections::HashMap, sync::OnceLock};

#[derive(Deserialize)]
struct Ratings {
    ratings: HashMap<u32, [u8; 2]>,
}

/// A deliberately qualitative ore-screening hint. It never produces a Boolean
/// breakability verdict: the actual mass, resistance, modules and range are unknown.
pub fn badge(catalog: &Catalog, value: u32, mode: Mode) -> Option<(String, u32)> {
    let found = catalog.resolve(value, mode);
    let [rock] = found.as_slice() else {
        return None;
    };
    if rock.kind != Kind::Rock {
        return None;
    }
    static RATINGS: OnceLock<Ratings> = OnceLock::new();
    let ratings = RATINGS.get_or_init(|| {
        serde_json::from_str(include_str!("../data/mining-difficulty.json"))
            .expect("bundled difficulty")
    });
    let Some([resistance, instability]) = ratings.ratings.get(&rock.base).copied() else {
        return Some(("?".into(), 0x00aaa299));
    };
    // Relative material burden only; never infer mass or ship capability.
    let burden = instability.max(resistance);
    let (label, color) = match burden {
        0 | 1 => ("LOW", 0x00cbd68a),
        2 => ("MEDIUM", 0x008bd9f3),
        _ => ("HIGH", 0x008c8bff),
    };
    Some((label.into(), color))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn never_claims_breakability_or_rates_an_ambiguous_target() {
        let c = Catalog::builtin();
        assert!(badge(&c, 8000, Mode::Auto).is_none());
        assert!(badge(&c, 18000, Mode::Auto).is_none());
        assert_eq!(badge(&c, 10800, Mode::Auto).unwrap().0, "HIGH");
        assert_eq!(badge(&c, 4300, Mode::Auto).unwrap().0, "LOW");
        assert_eq!(badge(&c, 3570, Mode::Auto).unwrap().0, "MEDIUM");
        assert_eq!(badge(&c, 3600, Mode::Ship), badge(&c, 10800, Mode::Ship));
        let custom = Catalog::from_json(
            r#"{"version":"custom","max_rocks":12,"resources":[{"name":"Custom","rs":3800}]}"#,
        )
        .unwrap();
        assert_eq!(badge(&custom, 3800, Mode::Ship).unwrap().0, "?");
    }
}
