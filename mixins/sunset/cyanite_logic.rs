// Pure parsing shared by the optional auction modules.
pub(super) fn crossed(before: f64, now: f64, level: f64, mode: &str) -> bool {
    let down = before >= level && now < level;
    let up = before <= level && now > level;
    match mode { "Going up" => up, "Either way" => up || down, _ => down }
}

pub(super) fn money(text: &str) -> Option<f64> {
    let text = text.trim().trim_start_matches('$').replace(',', "");
    let (number, scale) = match text.as_bytes().last()?.to_ascii_lowercase() {
        b'k' => (&text[..text.len() - 1], 1_000.0),
        b'm' => (&text[..text.len() - 1], 1_000_000.0),
        b'b' => (&text[..text.len() - 1], 1_000_000_000.0),
        _ => (text.as_str(), 1.0),
    };
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return None;
    }
    let value = number.parse::<f64>().ok()? * scale;
    (value.is_finite() && value > 0.0 && value <= 1_000_000_000_000.0).then_some(value)
}

pub(super) fn listing_price(line: &str) -> Option<f64> {
    let lower = line.to_ascii_lowercase();
    let at = lower.find("price")?;
    let rest = line[at + 5..].trim_start_matches(|c: char| !c.is_ascii_digit() && c != '-' && c != '+');
    let number = rest.split_whitespace().next()?;
    money(number)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn height_alerts_only_fire_when_crossing_the_selected_boundary() {
        assert!(crossed(0.0, -3.0, -2.0, "Going down"));
        assert!(!crossed(-3.0, -4.0, -2.0, "Going down"));
        assert!(!crossed(-3.0, 0.0, -2.0, "Going down"));
        assert!(crossed(-3.0, 0.0, -2.0, "Going up"));
        assert!(!crossed(0.0, -3.0, -2.0, "Going up"));
        assert!(crossed(-2.0, -3.0, -2.0, "Either way"));
        assert!(!crossed(-2.0, -2.0, -2.0, "Either way"));
    }

    #[test]
    fn auction_prices_require_a_price_label_and_a_valid_positive_amount() {
        assert_eq!(listing_price("Price: $1,250"), Some(1250.0));
        assert_eq!(listing_price("Total price: 1.5k coins"), Some(1500.0));
        assert_eq!(money("$2M"), Some(2_000_000.0));
        for line in ["Seller: player123", "Price: -3", "Price: NaN", "Price: infinity", "Price: 0", "Price: 1e5"] {
            assert_eq!(listing_price(line), None, "{line}");
        }
    }
}
