//! The encoding switch of a view.
//!
//! Exists to map a view's settings to the choices its switch offers (the four
//! encodings, plus pulse timings where the view allows them: design decisions
//! 19 and 20), which choice is current, and how a selection is stored back.

use ir_codec::Encoding;
use ir_settings::ViewSettings;

use crate::capture::Shown;

/// The choices of the switch, in display order.
pub fn options(view: &ViewSettings) -> Vec<Shown> {
    let mut all: Vec<Shown> = Encoding::ALL.into_iter().map(Shown::Encoded).collect();
    if view.offer_pulses {
        all.push(Shown::Pulses);
    }
    all
}

/// Label of a choice.
pub fn label(shown: Shown) -> &'static str {
    match shown {
        Shown::Encoded(Encoding::Hex) => "Hex",
        Shown::Encoded(Encoding::Binary) => "Binary",
        Shown::Encoded(Encoding::Decimal) => "Decimal",
        Shown::Encoded(Encoding::Base64) => "Base64",
        Shown::Pulses => "Pulses",
    }
}

/// The current choice.
pub fn current(view: &ViewSettings) -> Shown {
    if view.offer_pulses && view.show_pulses { Shown::Pulses } else { Shown::Encoded(view.encoding) }
}

/// Stores a choice; returns false when the view does not offer it.
pub fn select(view: &mut ViewSettings, shown: Shown) -> bool {
    match shown {
        Shown::Pulses if !view.offer_pulses => false,
        Shown::Pulses => {
            view.show_pulses = true;
            true
        }
        Shown::Encoded(encoding) => {
            view.encoding = encoding;
            view.show_pulses = false;
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENCODINGS: [Shown; 4] = [
        Shown::Encoded(Encoding::Hex),
        Shown::Encoded(Encoding::Binary),
        Shown::Encoded(Encoding::Decimal),
        Shown::Encoded(Encoding::Base64),
    ];

    #[test]
    fn options_with_and_without_pulses() {
        let mut view = ViewSettings::default();
        let mut all = ENCODINGS.to_vec();
        all.push(Shown::Pulses);
        assert_eq!(options(&view), all);
        view.offer_pulses = false;
        assert_eq!(options(&view), ENCODINGS.to_vec());
    }

    #[test]
    fn labels() {
        let labels: Vec<_> = options(&ViewSettings::default()).into_iter().map(label).collect();
        assert_eq!(labels, ["Hex", "Binary", "Decimal", "Base64", "Pulses"]);
    }

    #[test]
    fn current_follows_the_settings() {
        let mut view = ViewSettings { encoding: Encoding::Base64, ..ViewSettings::default() };
        assert_eq!(current(&view), Shown::Encoded(Encoding::Base64));
        view.show_pulses = true;
        assert_eq!(current(&view), Shown::Pulses);
        view.offer_pulses = false;
        assert_eq!(current(&view), Shown::Encoded(Encoding::Base64), "pulses not offered");
    }

    #[test]
    fn select_stores_the_choice() {
        let mut view = ViewSettings::default();
        assert!(select(&mut view, Shown::Pulses));
        assert!(view.show_pulses);
        assert!(select(&mut view, Shown::Encoded(Encoding::Decimal)));
        assert_eq!((view.encoding, view.show_pulses), (Encoding::Decimal, false));
        view.offer_pulses = false;
        assert!(!select(&mut view, Shown::Pulses));
        assert_eq!(current(&view), Shown::Encoded(Encoding::Decimal));
    }
}
