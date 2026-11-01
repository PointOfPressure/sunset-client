// Arguments for the first, walking-only navigation command.
pub(super) fn parse(args: &[&str]) -> Result<Option<[i32; 3]>, &'static str> {
    if args.len() == 1 && args[0].eq_ignore_ascii_case("stop") {
        return Ok(None);
    }
    let usage = "Use .goto <x> <y> <z> (integer block coordinates), or .goto stop.";
    if args.len() != 3 {
        return Err(usage);
    }
    let mut at = [0; 3];
    for (v, word) in at.iter_mut().zip(args) {
        *v = word.parse::<i32>().map_err(|_| usage)?;
    }
    if !(-29_999_983..=29_999_983).contains(&at[0]) || !(-29_999_983..=29_999_983).contains(&at[2]) || !(-64..=318).contains(&at[1]) {
        return Err("Destination is outside the walking world's coordinate limits.");
    }
    Ok(Some(at))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_block_coordinates() {
        assert_eq!(parse(&["-12", "-64", "35"]), Ok(Some([-12, -64, 35])));
    }
    #[test]
    fn stop_is_explicit_and_accepts_case() {
        assert_eq!(parse(&["STOP"]), Ok(None));
        assert!(parse(&["stop", "1"]).is_err());
    }
    #[test]
    fn rejects_missing_extra_and_non_integer_values() {
        for args in [
            vec![],
            vec!["1", "2"],
            vec!["1", "2", "3", "4"],
            vec!["1.5", "64", "2"],
            vec!["NaN", "64", "2"],
            vec!["inf", "64", "2"],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
    }
    #[test]
    fn rejects_overflow_and_world_edges_without_panicking() {
        for args in [
            ["2147483648", "64", "0"],
            ["-2147483648", "64", "0"],
            ["0", "319", "0"],
            ["0", "-65", "0"],
            ["0", "64", "30000000"],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
    }
}
