// Pure helpers: Depth Alert's height crossing, and the auction modules' price and button text.
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
    if at > 0 && lower.as_bytes()[at - 1].is_ascii_alphabetic() { return None; }
    // Only the total purchase price is useful for both stack and per-item limits.
    if ["unit", "each", "per item", "starting", "bid"].iter().any(|word| lower.contains(word)) { return None; }
    let rest = line[at + 5..].trim_start_matches(|c: char| c.is_whitespace() || matches!(c, ':' | '=' | '»' | '›'));
    let number = rest.split_whitespace().next()?;
    money(number)
}

pub(super) fn stable_listing_line(line: &str) -> bool {
    let line = line.to_lowercase();
    !["expires", "remaining", "time left", "ends in", "refresh"].iter().any(|word| line.contains(word))
}

pub(super) fn confirms(label: &str, wanted: &str) -> bool {
    let label = label.to_lowercase();
    let wanted = wanted.trim().to_lowercase();
    !wanted.is_empty() && label.contains(&wanted)
        && !label.split(|c: char| !c.is_alphabetic()).any(|w| matches!(w, "cancel" | "decline" | "deny" | "back" | "no" | "not"))
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
        for line in ["Seller: player123", "Price: -3", "Price: NaN", "Price: infinity", "Price: 0", "Price: 1e5",
            "Surprise: 5", "Unit price: 2", "Price per item: 2", "Price: not for sale 50", "Starting price: 1"] {
            assert_eq!(listing_price(line), None, "{line}");
        }
    }

    #[test]
    fn listing_identity_ignores_countdowns_but_keeps_seller_price_and_id() {
        for line in ["Expires in 9m", "Time left: 6m", "9 minutes remaining", "Ends in 1h"] {
            assert!(!stable_listing_line(line));
        }
        for line in ["Seller: Notch", "Price: $150k", "Listing ID: 1234"] {
            assert!(stable_listing_line(line));
        }
    }

    #[test]
    fn confirmation_buttons_require_a_positive_named_action() {
        assert!(confirms("CONFIRM PURCHASE", "confirm"));
        assert!(confirms("Buy now", "buy"));
        for label in ["Cancel", "Do not confirm", "Decline / Confirm", "No, go back"] {
            assert!(!confirms(label, "confirm"), "{label}");
        }
        assert!(!confirms("Confirm", ""));
    }
}
