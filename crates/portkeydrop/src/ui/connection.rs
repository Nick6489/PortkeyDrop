//! Track connection attempts independently of worker completion order.

#[derive(Default)]
pub struct ConnectionAttempts {
    generation: u64,
    active: Option<u64>,
}

impl ConnectionAttempts {
    pub fn begin(&mut self) -> Option<u64> {
        if self.active.is_some() {
            return None;
        }
        self.generation += 1;
        self.active = Some(self.generation);
        self.active
    }

    pub fn is_current(&self, attempt: u64) -> bool {
        self.active == Some(attempt)
    }

    pub fn finish(&mut self, attempt: u64) -> bool {
        if !self.is_current(attempt) {
            return false;
        }
        self.active = None;
        true
    }

    pub fn cancel(&mut self) -> bool {
        self.active.take().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_connect_starts_one_attempt() {
        let mut attempts = ConnectionAttempts::default();
        let first = attempts.begin().unwrap();
        assert_eq!(attempts.begin(), None);
        assert!(attempts.finish(first));
        assert_ne!(attempts.begin().unwrap(), first);
    }

    #[test]
    fn cancelled_workers_cannot_finish_a_new_attempt() {
        let mut attempts = ConnectionAttempts::default();
        let old = attempts.begin().unwrap();
        assert!(attempts.cancel());
        let current = attempts.begin().unwrap();
        assert!(!attempts.is_current(old));
        assert!(!attempts.finish(old));
        assert!(attempts.is_current(current));
        assert!(attempts.finish(current));
        assert!(!attempts.finish(current));
    }
}
