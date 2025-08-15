
#[derive(Clone, PartialEq)]
pub enum EffectDuration {
    Immediate,
    Persistent(Option<SmallTimer>),
    Continuous(Option<SmallTimer>),
    Repeating(RepeatingSmallTimer, Option<SmallTimer>),
}

#[derive(Clone, PartialEq)]
pub struct SmallTimer {
    pub(crate) remaining: f32,
}

impl SmallTimer {
    pub(crate) fn tick(&mut self, secs: f32) {
        self.remaining -= secs;
    }

    pub(crate) fn finished(&self) -> bool {
        self.remaining <= 0.
    }

    pub fn set_duration(&mut self, timer: impl Into<SmallTimer>) {
        self.remaining = timer.into().remaining;
    }
}

impl From<f32> for SmallTimer {
    fn from(value: f32) -> Self {
        Self { remaining: value }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct RepeatingSmallTimer {
    period: f32,
    pub(crate) remaining: f32,
    triggered: bool,
}

impl RepeatingSmallTimer {
    pub(crate) fn tick(&mut self, secs: f32) {
        self.remaining -= secs;
        if self.remaining <= 0. {
            self.remaining += self.period;
            self.triggered = true;
            self.remaining = f32::max(self.remaining, 0.);
        } else {
            self.triggered = false;
        }
    }

    pub(crate) fn just_triggered(&self) -> bool {
        self.triggered
    }

    pub fn set_duration(&mut self, timer: impl Into<RepeatingSmallTimer>) {
        self.remaining = timer.into().remaining;
    }
}

impl From<f32> for RepeatingSmallTimer {
    fn from(value: f32) -> Self {
        // We want the effect to start soon after being triggered
        // If it's a 10 second trigger we don't want to wait 10s
        // before the first trigger.  On the other hand we don't 
        // want to tick the timer immediately if it starts at 0.
        // I'm arbitrarily putting a 1s delay
        Self { period: 1., remaining: value, triggered: false }
    }
}
