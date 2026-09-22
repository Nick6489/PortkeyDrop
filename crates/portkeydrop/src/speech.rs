//! Speech output through [prismer](https://crates.io/crates/prismer).
//!
//! Construction never fails. When no backend is available, or the user has
//! not asked to be spoken to, announcements are silent no-ops.

use std::mem;

use prismer::{Backend, Features, Prism};

use portkeydrop_core::settings::is_screen_reader_backend;

/// Screen-reader announcement helper with a graceful fallback.
pub struct Announcer {
    // Declaration order is the drop order. The backend handle must be
    // released before the context that created it.
    backend: Option<Backend<'static>>,
    // Held so the context outlives every backend handle. Nothing reads the
    // field; dropping it is the point.
    #[allow(dead_code)]
    prism: Option<Box<Prism>>,
}

impl Announcer {
    /// Attach to the highest-priority backend that will start.
    ///
    /// That may be a screen reader or, when none is running, a system voice.
    /// Callers decide whether a system voice is allowed to speak.
    pub fn new() -> Self {
        let prism = match Prism::new() {
            Ok(prism) => Box::new(prism),
            Err(err) => {
                log::debug!("prism unavailable: {err}");
                return Self::disabled();
            }
        };
        // `Backend` borrows the context only in its type. Take it out of the
        // result and erase that lifetime before moving `prism`, or the result's
        // destructor keeps the borrow alive until the end of this function.
        let backend = match prism.create_best() {
            Ok(backend) => {
                log::info!("prism backend active: {}", backend.name());
                // The handle is a pointer owned by the backend value, and this
                // struct drops that value before the context.
                let backend: Backend<'static> = unsafe { mem::transmute(backend) };
                Some(backend)
            }
            Err(err) => {
                log::debug!("no prism backend available: {err}");
                None
            }
        };
        Self {
            backend,
            prism: Some(prism),
        }
    }

    /// An announcer that never speaks. Used in tests and headless runs.
    pub fn disabled() -> Self {
        Self {
            backend: None,
            prism: None,
        }
    }

    /// Whether a backend that can speak was started.
    pub fn has_backend(&self) -> bool {
        self.backend.as_ref().is_some_and(|backend| {
            backend.supports(Features::SPEAK) || backend.supports(Features::OUTPUT)
        })
    }

    /// Name of the backend Prism selected, if any.
    pub fn backend_name(&self) -> Option<String> {
        self.backend.as_ref().map(Backend::name)
    }

    /// Whether that backend is a screen reader rather than a system voice.
    pub fn is_screen_reader(&self) -> bool {
        self.backend_name()
            .is_some_and(|name| is_screen_reader_backend(&name))
    }

    /// Whether the selected backend accepts a speech rate.
    ///
    /// SAPI and OneCore do. NVDA and JAWS do not: they keep the rate the
    /// reader is already using.
    pub fn accepts_rate(&self) -> bool {
        self.backend
            .as_ref()
            .is_some_and(|backend| backend.supports(Features::SET_RATE))
    }

    /// Whether the selected backend accepts a speech volume.
    pub fn accepts_volume(&self) -> bool {
        self.backend
            .as_ref()
            .is_some_and(|backend| backend.supports(Features::SET_VOLUME))
    }

    /// Speak `text`, interrupting whatever is already being said.
    ///
    /// Returns whether the text reached a backend. Screen readers that also
    /// drive a braille display receive the text there too.
    pub fn announce(&mut self, text: &str) -> bool {
        let Some(backend) = self.backend.as_ref() else {
            return false;
        };
        let result = if backend.supports(Features::OUTPUT) {
            backend.output(text, true)
        } else {
            backend.speak(text, true)
        };
        match result {
            Ok(()) => true,
            Err(err) => {
                log::warn!("failed to announce text via prism: {err}");
                false
            }
        }
    }

    /// Apply the app's 0–100 speech settings to a backend that accepts them.
    pub fn apply_settings(&mut self, rate: Option<i32>, volume: Option<i32>) {
        let Some(backend) = self.backend.as_ref() else {
            return;
        };
        if let Some(rate) = rate.filter(|_| backend.supports(Features::SET_RATE)) {
            if let Err(err) = backend.set_rate(percent_to_fraction(rate)) {
                log::debug!("backend does not accept a speech rate: {err}");
            }
        }
        if let Some(volume) = volume.filter(|_| backend.supports(Features::SET_VOLUME)) {
            if let Err(err) = backend.set_volume(percent_to_fraction(volume)) {
                log::debug!("backend does not accept a speech volume: {err}");
            }
        }
    }
}

impl Default for Announcer {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Announcer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Announcer")
            .field("backend", &self.backend_name())
            .field("screen_reader", &self.is_screen_reader())
            .finish()
    }
}

/// Convert a 0–100 setting into the 0.0–1.0 fraction Prism expects.
fn percent_to_fraction(percent: i32) -> f32 {
    percent.clamp(0, 100) as f32 / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_to_fraction_clamps_and_scales() {
        assert_eq!(percent_to_fraction(0), 0.0);
        assert_eq!(percent_to_fraction(50), 0.5);
        assert_eq!(percent_to_fraction(100), 1.0);
        assert_eq!(percent_to_fraction(-20), 0.0);
        assert_eq!(percent_to_fraction(250), 1.0);
    }

    #[test]
    fn disabled_announcer_is_silent() {
        let mut announcer = Announcer::disabled();
        assert!(!announcer.has_backend());
        assert!(!announcer.is_screen_reader());
        assert!(!announcer.accepts_rate());
        assert!(!announcer.announce("hello"));
        announcer.apply_settings(Some(70), Some(90));
    }
}
