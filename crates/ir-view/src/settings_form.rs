//! The Settings page form.
//!
//! Exists to edit the user settings with validation: the global carrier and
//! repeat defaults (design decisions 12, 13), the capture limit (15) and the
//! display options shared by every view's encoding switch (19, 20:
//! uppercase hex, bytes per line, whether pulse timings are offered). Each
//! view keeps its own selected encoding.

use ir_library::model::RepeatMode;
use ir_settings::Settings;

use crate::send::parse_carrier;

/// Largest bytes-per-line value accepted.
pub const MAX_BYTES_PER_LINE: usize = 1024;

/// The fields as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsForm {
    pub carrier_text: String,
    pub repeat: RepeatMode,
    pub limit_mib_text: String,
    pub uppercase: bool,
    pub bytes_per_line_text: String,
    pub offer_pulses: bool,
}

impl SettingsForm {
    pub fn from_settings(settings: &Settings) -> Self {
        let view = settings.views.capture;
        Self {
            carrier_text: settings.send.carrier_hz.to_string(),
            repeat: settings.send.repeat,
            limit_mib_text: settings.capture.limit_mib.to_string(),
            uppercase: view.uppercase,
            bytes_per_line_text: view.bytes_per_line.to_string(),
            offer_pulses: view.offer_pulses,
        }
    }

    /// Writes the form into `settings`; leaves them unchanged on an error.
    pub fn apply(&self, settings: &mut Settings) -> Result<(), String> {
        let carrier_hz = parse_carrier(&self.carrier_text)?.ok_or("the default carrier is missing")?;
        let limit_mib = self
            .limit_mib_text
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|&mib| mib >= 1)
            .ok_or("the capture limit must be a whole number of MiB, at least 1")?;
        let bytes_per_line = self
            .bytes_per_line_text
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|&n| n <= MAX_BYTES_PER_LINE)
            .ok_or_else(|| format!("bytes per line must be between 0 and {MAX_BYTES_PER_LINE}"))?;
        if self.repeat == (RepeatMode::FirstFrame { count: 0 }) {
            return Err("the repeat count must be at least 1".into());
        }
        settings.send.carrier_hz = carrier_hz;
        settings.send.repeat = self.repeat;
        settings.capture.limit_mib = limit_mib;
        let views = &mut settings.views;
        for view in [&mut views.capture, &mut views.transfers, &mut views.send, &mut views.library] {
            view.uppercase = self.uppercase;
            view.bytes_per_line = bytes_per_line;
            view.offer_pulses = self.offer_pulses;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ir_codec::Encoding;

    #[test]
    fn form_shows_the_settings() {
        let mut s = Settings::default();
        s.send.repeat = RepeatMode::FirstFrame { count: 2 };
        s.views.capture.bytes_per_line = 8;
        assert_eq!(
            SettingsForm::from_settings(&s),
            SettingsForm {
                carrier_text: "38000".into(),
                repeat: RepeatMode::FirstFrame { count: 2 },
                limit_mib_text: "4096".into(),
                uppercase: true,
                bytes_per_line_text: "8".into(),
                offer_pulses: true,
            }
        );
    }

    #[test]
    fn apply_sets_defaults_limit_and_every_view() {
        let mut s = Settings::default();
        s.views.send.encoding = Encoding::Base64;
        let form = SettingsForm {
            carrier_text: " 36000 ".into(),
            repeat: RepeatMode::AsLearned,
            limit_mib_text: "512".into(),
            uppercase: false,
            bytes_per_line_text: "0".into(),
            offer_pulses: false,
        };
        form.apply(&mut s).unwrap();
        assert_eq!((s.send.carrier_hz, s.send.repeat, s.capture.limit_mib), (36_000, RepeatMode::AsLearned, 512));
        for view in [s.views.capture, s.views.transfers, s.views.send, s.views.library] {
            assert_eq!((view.uppercase, view.bytes_per_line, view.offer_pulses), (false, 0, false));
        }
        assert_eq!(s.views.send.encoding, Encoding::Base64, "the selected encoding stays");
    }

    #[test]
    fn invalid_fields_leave_the_settings_unchanged() {
        let base = SettingsForm::from_settings(&Settings::default());
        for (form, message) in [
            (SettingsForm { carrier_text: "".into(), ..base.clone() }, "the default carrier is missing"),
            (SettingsForm { carrier_text: "0".into(), ..base.clone() }, "carrier: must be between 1 and 16252928 Hz"),
            (SettingsForm { limit_mib_text: "0".into(), ..base.clone() }, "the capture limit must be a whole number of MiB, at least 1"),
            (SettingsForm { limit_mib_text: "lots".into(), ..base.clone() }, "the capture limit must be a whole number of MiB, at least 1"),
            (SettingsForm { bytes_per_line_text: "2000".into(), ..base.clone() }, "bytes per line must be between 0 and 1024"),
            (SettingsForm { repeat: RepeatMode::FirstFrame { count: 0 }, ..base.clone() }, "the repeat count must be at least 1"),
        ] {
            let mut s = Settings::default();
            assert_eq!(form.apply(&mut s), Err(message.to_string()));
            assert_eq!(s, Settings::default());
        }
    }
}
