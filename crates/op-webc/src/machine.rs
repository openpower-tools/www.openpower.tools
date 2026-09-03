//! A pure interaction machine for switch-like controls whose change
//! takes time and can be cancelled.
//!
//! Every interaction state that participates in something timed is
//! explicit here, and the DOM layer is a thin translation: pointer,
//! focus, activation and "the transition finished" arrive as
//! [`Input`]s; attribute, state and stylesheet changes leave as
//! [`Effect`]s. Because the machine is plain data, every (state, input)
//! pair is enumerable and tested, and a report can draw it.
//!
//! The machine tracks three things: the setting (`on`), whether the
//! control has attention (pointer or visible focus), and the flight:
//! `Idle`, `Toward` a new setting, or `Back` after a mid-flight
//! activation reverted it. Attention is a flag rather than a state
//! because it is orthogonal to the flight; the stylesheet decides that
//! a preview shows only when attention is present AND no flight is
//! running.

/// What the world tells the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// The pointer arrived or visible focus landed.
    Attend,
    /// Both pointer and visible focus are gone.
    Neglect,
    /// A click, or keyboard activation.
    Activate,
    /// The timed change (the palette blend, and with it the progress
    /// ghost) completed - forward or reversed.
    Finished,
}

/// Where a change is, if one is running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flight {
    Idle,
    /// Travelling to a new setting.
    Toward,
    /// Returning to the previous setting after an abort.
    Back,
}

/// What the machine asks the element to do, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Reflect and persist the setting; the solid thumb shows it at once.
    SetOn(bool),
    /// Expose attention (the preview plays while idle).
    Attention(bool),
    /// Arm the timed change: palette blend and progress ghost run on
    /// the blend clock from here.
    Arm,
    /// Disarm it: clocks return to instant.
    Disarm,
    /// Update the accessible description.
    Describe(Description),
}

/// Which accessible description applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Description {
    /// "Setting: X. Activate to switch to Y."
    Settled,
    /// "Switching to X. Activate to return to Y."
    Switching,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Machine {
    pub on: bool,
    pub attention: bool,
    pub flight: Flight,
}

impl Machine {
    pub fn new(on: bool) -> Self {
        Self {
            on,
            attention: false,
            flight: Flight::Idle,
        }
    }

    pub fn in_flight(&self) -> bool {
        self.flight != Flight::Idle
    }

    /// Applies one input and returns the effects to perform, in order.
    pub fn on(&mut self, input: Input) -> Vec<Effect> {
        use Effect::*;
        match (input, self.flight) {
            (Input::Attend, _) => {
                self.attention = true;
                vec![Attention(true)]
            }
            (Input::Neglect, _) => {
                self.attention = false;
                vec![Attention(false)]
            }
            (Input::Activate, Flight::Idle) => {
                self.on = !self.on;
                self.flight = Flight::Toward;
                vec![Arm, SetOn(self.on), Describe(Description::Switching)]
            }
            (Input::Activate, Flight::Toward) => {
                // Abort: the setting returns at once; the armed clocks
                // reverse the palette and the progress ghost.
                self.on = !self.on;
                self.flight = Flight::Back;
                vec![SetOn(self.on), Describe(Description::Settled)]
            }
            (Input::Activate, Flight::Back) => {
                // Mind changed again: a fresh flight from wherever the
                // reversal had got to.
                self.on = !self.on;
                self.flight = Flight::Toward;
                vec![SetOn(self.on), Describe(Description::Switching)]
            }
            (Input::Finished, Flight::Idle) => vec![],
            (Input::Finished, _) => {
                self.flight = Flight::Idle;
                vec![Disarm, Describe(Description::Settled)]
            }
        }
    }
}

/// Every input, for enumeration.
pub const INPUTS: [Input; 4] = [
    Input::Attend,
    Input::Neglect,
    Input::Activate,
    Input::Finished,
];
/// Every flight state, for enumeration.
pub const FLIGHTS: [Flight; 3] = [Flight::Idle, Flight::Toward, Flight::Back];

/// One row of the full transition table: `(from, input, to, effects)`.
pub type Transition = (Machine, Input, Machine, Vec<Effect>);

/// The complete transition table, derived from [`Machine::on`] itself so
/// it cannot disagree with the behaviour. Reports draw from this.
pub fn table() -> Vec<Transition> {
    let mut rows = Vec::new();
    for on in [false, true] {
        for attention in [false, true] {
            for flight in FLIGHTS {
                for input in INPUTS {
                    let from = Machine {
                        on,
                        attention,
                        flight,
                    };
                    let mut to = from;
                    let effects = to.on(input);
                    rows.push((from, input, to, effects));
                }
            }
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use Effect::*;

    #[test]
    fn every_state_handles_every_input() {
        // 2 x 2 x 3 states x 4 inputs; no (state, input) pair is a
        // silent fallthrough: each either changes state or emits.
        let rows = table();
        assert_eq!(rows.len(), 48);
        for (from, input, to, effects) in &rows {
            let inert = from == to && effects.is_empty();
            assert!(
                !inert || *input == Input::Finished && from.flight == Flight::Idle,
                "{from:?} + {input:?} silently does nothing"
            );
        }
    }

    #[test]
    fn a_click_flips_the_setting_arms_the_clocks_and_describes_the_flight() {
        let mut m = Machine::new(false);
        assert_eq!(
            m.on(Input::Activate),
            vec![Arm, SetOn(true), Describe(Description::Switching)]
        );
        assert_eq!(m.flight, Flight::Toward);
        assert!(m.on);
    }

    #[test]
    fn a_second_click_mid_flight_aborts_without_rearming() {
        let mut m = Machine::new(false);
        m.on(Input::Activate);
        let effects = m.on(Input::Activate);
        assert_eq!(effects, vec![SetOn(false), Describe(Description::Settled)]);
        assert!(!effects.contains(&Arm) && !effects.contains(&Disarm));
        assert_eq!(m.flight, Flight::Back);
        assert!(!m.on, "the setting is back where it started");
    }

    #[test]
    fn a_third_click_starts_a_fresh_flight_from_the_reversal() {
        let mut m = Machine::new(false);
        m.on(Input::Activate);
        m.on(Input::Activate);
        let effects = m.on(Input::Activate);
        assert_eq!(effects, vec![SetOn(true), Describe(Description::Switching)]);
        assert_eq!(m.flight, Flight::Toward);
        assert!(m.on);
    }

    #[test]
    fn finished_disarms_and_settles_whatever_the_direction() {
        for clicks in [1, 2, 3] {
            let mut m = Machine::new(false);
            for _ in 0..clicks {
                m.on(Input::Activate);
            }
            let effects = m.on(Input::Finished);
            assert_eq!(effects, vec![Disarm, Describe(Description::Settled)]);
            assert_eq!(m.flight, Flight::Idle);
            assert_eq!(m.on, clicks % 2 == 1);
        }
    }

    #[test]
    fn finished_while_idle_is_a_no_op() {
        let mut m = Machine::new(true);
        assert!(m.on(Input::Finished).is_empty());
        assert_eq!(m, Machine::new(true));
    }

    #[test]
    fn attention_is_orthogonal_to_the_flight() {
        let mut m = Machine::new(false);
        assert_eq!(m.on(Input::Attend), vec![Attention(true)]);
        m.on(Input::Activate);
        assert!(m.attention && m.in_flight());
        assert_eq!(m.on(Input::Neglect), vec![Attention(false)]);
        assert!(m.in_flight(), "losing attention never touches the flight");
        assert_eq!(m.on(Input::Attend), vec![Attention(true)]);
        m.on(Input::Finished);
        assert!(m.attention, "attention survives the settle");
    }

    /// Regardless of interleaving, the setting equals the start flipped
    /// once per activation - abort and re-fly included.
    #[test]
    fn the_setting_is_the_parity_of_activations_under_any_interleaving() {
        let mut seed: u32 = 0x2545_F491;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for _ in 0..500 {
            let start = next() % 2 == 0;
            let mut m = Machine::new(start);
            let mut activations = 0;
            for _ in 0..(next() % 12) {
                let input = INPUTS[(next() % 4) as usize];
                if input == Input::Activate {
                    activations += 1;
                }
                m.on(input);
            }
            assert_eq!(m.on, start ^ (activations % 2 == 1));
            assert_eq!(m.in_flight(), false || m.flight != Flight::Idle);
        }
    }
}
