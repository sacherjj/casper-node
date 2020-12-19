use std::{cmp::max, collections::VecDeque, mem};

use datasize::DataSize;
use tracing::trace;

use crate::{
    components::consensus::{
        highway_core::{finality_detector::FinalityDetector, round_id, State, Weight},
        traits::Context,
    },
    types::Timestamp,
};

// The number of most recent rounds we will be keeping track of.
const NUM_ROUNDS_TO_CONSIDER: usize = 40;
// The maximum number of failures allowed among NUM_ROUNDS_TO_CONSIDER latest rounds, with which we
// won't increase our round length. Exceeding this threshold will mean that we should slow down.
const MAX_FAILED_ROUNDS: usize = 10;
// We will try to accelerate (decrease our round exponent) every `ACCELERATION_PARAMETER` rounds if
// we have few enough failures.
const ACCELERATION_PARAMETER: u64 = 40;
// The maximum number of failures with which we will attempt to accelerate (decrease the round
// exponent).
const MAX_FAILURES_FOR_ACCELERATION: usize = 3;
// The FTT, as a percentage (i.e. `THRESHOLD = 1` means 1% of the validators' total weight), which
// we will use for looking for a summit in order to determine a proposal's finality.
// The required quorum in a summit we will look for to check if a round was successful is
// determined by this FTT.
const THRESHOLD: u64 = 1;

#[derive(DataSize, Debug)]
pub(crate) struct RoundSuccessMeter<C>
where
    C: Context,
{
    // store whether a particular round was successful
    // index 0 is the last handled round, 1 is the second-to-last etc.
    rounds: VecDeque<bool>,
    current_round_id: u64,
    proposals: Vec<C::Hash>,
    min_round_exp: u8,
    max_round_exp: u8,
    current_round_exp: u8,
}

impl<C: Context> RoundSuccessMeter<C> {
    pub fn new(round_exp: u8, min_round_exp: u8, max_round_exp: u8, timestamp: Timestamp) -> Self {
        let current_round_id = round_id(timestamp, round_exp).millis();
        Self {
            rounds: VecDeque::with_capacity(NUM_ROUNDS_TO_CONSIDER),
            current_round_id,
            proposals: Vec::new(),
            min_round_exp,
            max_round_exp,
            current_round_exp: round_exp,
        }
    }

    fn change_exponent(&mut self, new_exp: u8, timestamp: Timestamp) {
        self.rounds = VecDeque::with_capacity(NUM_ROUNDS_TO_CONSIDER);
        self.current_round_exp = new_exp;
        self.current_round_id = round_id(timestamp, new_exp).millis();
        self.proposals = Vec::new();
    }

    fn check_proposals_success(&self, state: &State<C>, proposal_h: &C::Hash) -> bool {
        let total_w = state.total_weight();

        let finality_detector =
            FinalityDetector::<C>::new(max(total_w / 100 * THRESHOLD, Weight(1)));

        // check for the existence of a level-1 summit
        finality_detector.find_summit(1, proposal_h, state) == 1
    }

    /// Registers a proposal within this round - if it's finalized within the round, the round will
    /// be successful.
    pub fn new_proposal(&mut self, proposal_h: C::Hash, timestamp: Timestamp) {
        // only add proposals from within the current round
        if round_id(timestamp, self.current_round_exp).millis() == self.current_round_id {
            trace!(
                %self.current_round_id,
                timestamp = timestamp.millis(),
                "adding a proposal"
            );
            self.proposals.push(proposal_h);
        } else {
            trace!(
                %self.current_round_id,
                timestamp = timestamp.millis(),
                %self.current_round_exp,
                "trying to add proposal for a different round!"
            );
        }
    }

    /// If the current timestamp indicates that the round has ended, checks the known proposals for
    /// a level-1 summit.
    /// If there is a summit, the round is considered successful. Otherwise, it is considered
    /// failed.
    /// Next, a number of last rounds are being checked for success and if not enough of them are
    /// successful, we return a higher round exponent for the future.
    /// If the exponent shouldn't grow, and the round ID is divisible by a certain number, a lower
    /// round exponent is returned.
    pub fn calculate_new_exponent(&mut self, state: &State<C>) -> u8 {
        let now = Timestamp::now();
        // if the round hasn't finished, just return whatever we have now
        if round_id(now, self.current_round_exp).millis() <= self.current_round_id {
            return self.new_exponent();
        }

        trace!(%self.current_round_id, "calculating exponent");
        let current_round_index = self.current_round_id >> self.current_round_exp;
        let new_round_index = now.millis() >> self.current_round_exp;

        if mem::take(&mut self.proposals)
            .into_iter()
            .any(|proposal| self.check_proposals_success(state, &proposal))
        {
            trace!("round succeeded");
            self.rounds.push_front(true);
        } else {
            trace!("round failed");
            self.rounds.push_front(false);
        }

        // if we're just switching rounds and more than a single round has passed, all the
        // rounds since the last registered round have failed
        for _ in 0..(new_round_index - current_round_index - 1) {
            trace!("round failed");
            self.rounds.push_front(false);
        }

        self.current_round_id = new_round_index << self.current_round_exp;

        self.clean_old_rounds();

        trace!(
            %self.current_round_exp,
            "{} failures among the last {} rounds.",
            self.count_failures(),
            self.rounds.len()
        );

        let new_exp = self.new_exponent();

        trace!(%new_exp, "new exponent calculated");

        if new_exp != self.current_round_exp {
            self.change_exponent(new_exp, now);
        }

        new_exp
    }

    fn clean_old_rounds(&mut self) {
        while self.rounds.len() > NUM_ROUNDS_TO_CONSIDER {
            self.rounds.pop_back();
        }
    }

    fn count_failures(&self) -> usize {
        self.rounds.iter().filter(|&success| !success).count()
    }

    fn new_exponent(&self) -> u8 {
        let current_round_index = self.current_round_id >> self.current_round_exp;
        let num_failures = self.count_failures();
        if num_failures > MAX_FAILED_ROUNDS && self.current_round_exp < self.max_round_exp {
            self.current_round_exp + 1
        } else if current_round_index % ACCELERATION_PARAMETER == 0
            && self.current_round_exp > self.min_round_exp
            // we will only accelerate if we collected data about enough rounds
            && self.rounds.len() == NUM_ROUNDS_TO_CONSIDER
            && num_failures < MAX_FAILURES_FOR_ACCELERATION
        {
            self.current_round_exp - 1
        } else {
            self.current_round_exp
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::components::consensus::{
        cl_context::ClContext,
        protocols::highway::round_success_meter::{
            ACCELERATION_PARAMETER, MAX_FAILED_ROUNDS, NUM_ROUNDS_TO_CONSIDER,
        },
    };

    const TEST_ROUND_EXP: u8 = 13;
    const TEST_MIN_ROUND_EXP: u8 = 8;
    const TEST_MAX_ROUND_EXP: u8 = 19;

    #[test]
    fn new_exponent_steady() {
        let round_success_meter: super::RoundSuccessMeter<ClContext> =
            super::RoundSuccessMeter::new(
                TEST_ROUND_EXP,
                TEST_MIN_ROUND_EXP,
                TEST_MAX_ROUND_EXP,
                crate::types::Timestamp::now(),
            );
        assert_eq!(round_success_meter.new_exponent(), TEST_ROUND_EXP);
    }

    #[test]
    fn new_exponent_slow_down() {
        let mut round_success_meter: super::RoundSuccessMeter<ClContext> =
            super::RoundSuccessMeter::new(
                TEST_ROUND_EXP,
                TEST_MIN_ROUND_EXP,
                TEST_MAX_ROUND_EXP,
                crate::types::Timestamp::now(),
            );
        // If there have been more rounds of failure than MAX_FAILED_ROUNDS, slow down
        round_success_meter.rounds = vec![false; MAX_FAILED_ROUNDS + 1].into();
        assert_eq!(round_success_meter.new_exponent(), TEST_ROUND_EXP + 1);
    }

    #[test]
    fn new_exponent_can_not_slow_down_because_max_round_exp() {
        // If the round exponent is the same as the maximum round exponent, can't go up
        let mut round_success_meter: super::RoundSuccessMeter<ClContext> =
            super::RoundSuccessMeter::new(
                TEST_MAX_ROUND_EXP,
                TEST_MIN_ROUND_EXP,
                TEST_MAX_ROUND_EXP,
                crate::types::Timestamp::now(),
            );
        // If there have been more rounds of failure than MAX_FAILED_ROUNDS, slow down -- but can't
        // slow down because of ceiling
        round_success_meter.rounds = vec![false; MAX_FAILED_ROUNDS + 1].into();
        assert_eq!(round_success_meter.new_exponent(), TEST_MAX_ROUND_EXP);
    }

    #[test]
    fn new_exponent_speed_up() {
        // If there's been enough successful rounds and it's an acceleration round, speed up
        let mut round_success_meter: super::RoundSuccessMeter<ClContext> =
            super::RoundSuccessMeter::new(
                TEST_ROUND_EXP,
                TEST_MIN_ROUND_EXP,
                TEST_MAX_ROUND_EXP,
                crate::types::Timestamp::now(),
            );
        round_success_meter.rounds = vec![true; NUM_ROUNDS_TO_CONSIDER].into();
        // Increase our round index until we are at an acceleration round
        loop {
            let current_round_index =
                round_success_meter.current_round_id >> round_success_meter.current_round_exp;
            if current_round_index % ACCELERATION_PARAMETER == 0 {
                break;
            };
            round_success_meter.current_round_id += 1;
        }
        assert_eq!(round_success_meter.new_exponent(), TEST_ROUND_EXP - 1);
    }

    #[test]
    fn new_exponent_can_not_speed_up_because_min_round_exp() {
        // If there's been enough successful rounds and it's an acceleration round, but we are
        // already at the smallest round exponent possible, stay at the current round exponent
        let mut round_success_meter: super::RoundSuccessMeter<ClContext> =
            super::RoundSuccessMeter::new(
                TEST_MIN_ROUND_EXP,
                TEST_MIN_ROUND_EXP,
                TEST_MAX_ROUND_EXP,
                crate::types::Timestamp::now(),
            );
        round_success_meter.rounds = vec![true; NUM_ROUNDS_TO_CONSIDER].into();
        // Increase our round index until we are at an acceleration round
        loop {
            let current_round_index =
                round_success_meter.current_round_id >> round_success_meter.current_round_exp;
            if current_round_index % ACCELERATION_PARAMETER == 0 {
                break;
            };
            round_success_meter.current_round_id += 1;
        }
        assert_eq!(round_success_meter.new_exponent(), TEST_MIN_ROUND_EXP);
    }
}
