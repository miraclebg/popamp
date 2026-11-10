//! Popamp's own version, and `min_popamp`: the oldest Popamp a skin or
//! plug-in says it works with. It is read before anything else in its
//! file, so a newer skin is refused for its version and not for a key this
//! Popamp does not know.

/// This Popamp's version: the whole workspace shares one.
pub const POPAMP: &str = env!("CARGO_PKG_VERSION");

/// `major.minor.patch`.
fn parse(v: &str) -> Option<(u32, u32, u32)> {
    let mut parts = v.split('.').map(|p| p.parse::<u32>().ok().filter(|_| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())));
    let v = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(v)
}

/// Ok when this Popamp is `min` or newer.
pub fn check_min(min: &str) -> Result<(), String> {
    check_min_against(min, POPAMP)
}

fn check_min_against(min: &str, this: &str) -> Result<(), String> {
    let need = parse(min).ok_or_else(|| format!("min_popamp {min:?} is not a version like \"0.8.0\""))?;
    match parse(this) {
        Some(have) if have < need => Err(format!("needs Popamp {min} or newer; this is {this}")),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_or_a_newer_popamp_is_enough() {
        assert_eq!(check_min_against("0.8.0", "0.8.0"), Ok(()));
        assert_eq!(check_min_against("0.8.0", "0.8.1"), Ok(()));
        assert_eq!(check_min_against("0.8.0", "0.10.0"), Ok(()), "compared as numbers, not text");
        assert_eq!(check_min_against("0.8.0", "1.0.0"), Ok(()));
        assert_eq!(check_min_against("0.9.0", "0.8.5").unwrap_err(), "needs Popamp 0.9.0 or newer; this is 0.8.5");
        assert_eq!(check_min_against("0.8.1", "0.8.0").unwrap_err(), "needs Popamp 0.8.1 or newer; this is 0.8.0");
    }

    #[test]
    fn a_version_is_three_numbers() {
        for bad in ["0.8", "0.8.0.1", "v0.8.0", "0.8.x", "", "0..8", "0.8.+1"] {
            assert!(check_min_against(bad, "0.8.0").unwrap_err().contains("is not a version"), "{bad}");
        }
        assert!(check_min(POPAMP).is_ok());
    }
}
