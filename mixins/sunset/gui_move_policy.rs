// GUI Move's stop is complete only after a quiet movement really left the client.
pub(super) fn ready(
    now: u64,
    quiet_move: Option<u64>,
    flags: u8,
    sprinting: bool,
    speed: f64,
) -> bool {
    quiet_move.is_some_and(|tick| tick < now)
        && flags & 0x7f == 0
        && !sprinting
        && speed.is_finite()
        && speed <= 0.003
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_for_an_earlier_forwarded_idle_move_and_momentum() {
        assert!(!ready(10, None, 0, false, 0.0));
        assert!(!ready(10, Some(10), 0, false, 0.0));
        assert!(!ready(10, Some(9), 64, false, 0.0));
        assert!(!ready(10, Some(9), 0, true, 0.0));
        assert!(!ready(10, Some(9), 0, false, 0.1));
        assert!(ready(10, Some(9), 0, false, 0.0));
    }
}
