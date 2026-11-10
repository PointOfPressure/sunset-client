// Access to the private native click counters, kept in their owning module.
pub fn counts(input: &super::InteractionInput) -> [u32; 3] { input.clicks }

pub fn cancel(input: &mut super::InteractionInput, owned: [u32; 3]) {
    for (pending, ours) in input.clicks.iter_mut().zip(owned) {
        *pending = pending.saturating_sub(ours);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_cancels_owned_clicks_and_preserves_physical_clicks() {
        let mut input = super::super::InteractionInput::default();
        for button in [super::super::Button::Attack, super::super::Button::Use,
            super::super::Button::Use, super::super::Button::Pick] { input.press(button); }
        cancel(&mut input, [0, 1, 0]);
        assert_eq!(counts(&input), [1, 1, 1]);
        cancel(&mut input, [0, 1, 0]);
        assert_eq!(counts(&input), [1, 0, 1]);
    }
}
