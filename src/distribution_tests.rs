//! Correctness tests for `distribute`.
//!
//! Add `#[cfg(test)] mod distribution_tests;` to `lib.rs` and place this file
//! next to it. These tests intentionally do not use the reference solver.

use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use super::{
    distribute, FractivityDistributionFailReason, FractivityEntry, FractivityRoom,
};

type InputEntry = (FractivityEntry, Option<(Uuid, i32)>);
type Assignment = (FractivityEntry, Uuid, i32);

fn uid(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

fn room(id: u128, priority: i32) -> FractivityRoom {
    FractivityRoom {
        id: uid(id),
        priority,
    }
}

fn entry(
    id: u128,
    instructors: &[u128],
    visitors: &[u128],
    duration: i32,
    allowed_rooms: &[u128],
    allowed_starts: &[i32],
    preparation_time: i32,
    follow_up_time: i32,
    blocker: bool,
) -> FractivityEntry {
    FractivityEntry {
        id: uid(id),
        topic: format!("entry-{id}"),
        instructor_extension_uuids: instructors.iter().copied().map(uid).collect(),
        visitor_extension_uuids: visitors.iter().copied().map(uid).collect(),
        duration,
        allowed_rooms: allowed_rooms.iter().copied().map(uid).collect(),
        allowed_starts: allowed_starts.to_vec(),
        preparation_time,
        follow_up_time,
        blocker,
    }
}

fn unpinned(e: FractivityEntry) -> InputEntry {
    (e, None)
}

fn pinned(e: FractivityEntry, room_id: u128, start: i32) -> InputEntry {
    (e, Some((uid(room_id), start)))
}

fn solve(
    entries: &[InputEntry],
    rooms: Vec<FractivityRoom>,
) -> Result<Vec<Assignment>, Vec<(FractivityEntry, FractivityDistributionFailReason)>> {
    distribute(&entries.to_vec(), rooms).expect("the solver itself should not return an error")
}

/// Independent validation of a successful result against the public input.
/// It deliberately does not inspect the MILP's variables or constraints.
fn assert_valid_distribution(entries: &[InputEntry], assignments: &[Assignment]) {
    let inputs_by_id: HashMap<Uuid, (&FractivityEntry, &Option<(Uuid, i32)>)> = entries
        .iter()
        .map(|(e, pin)| (e.id, (e, pin)))
        .collect();

    assert_eq!(inputs_by_id.len(), entries.len(), "input IDs must be unique");
    assert_eq!(
        assignments.len(),
        entries.len(),
        "a successful distribution must assign every input entry exactly once"
    );

    let mut seen = HashSet::new();
    for (returned_entry, assigned_room, assigned_start) in assignments {
        assert!(
            seen.insert(returned_entry.id),
            "entry {:?} appears more than once in the output",
            returned_entry.id
        );

        let (original, pin) = *inputs_by_id
            .get(&returned_entry.id)
            .unwrap_or_else(|| panic!("unknown entry {:?} in output", returned_entry.id));

        assert_eq!(
            *returned_entry, *original,
            "the returned activity metadata should match the original input"
        );
        assert!(
            original.allowed_rooms.contains(assigned_room),
            "entry {:?} assigned to a disallowed room {:?}",
            original.id,
            assigned_room
        );

        if let Some((pinned_room, pinned_start)) = pin {
            assert_eq!(assigned_room, pinned_room, "pinned room was not respected");
            assert_eq!(assigned_start, pinned_start, "pinned start was not respected");
        } else {
            let min_start = *original
                .allowed_starts
                .iter()
                .min()
                .expect("successful inputs must have at least one start");
            let max_start = *original
                .allowed_starts
                .iter()
                .max()
                .expect("successful inputs must have at least one start");
            assert!(
                *assigned_start >= min_start && *assigned_start <= max_start,
                "entry {:?} start {} is outside [{}, {}]",
                original.id,
                assigned_start,
                min_start,
                max_start
            );
        }
    }

    assert_eq!(seen.len(), inputs_by_id.len(), "some input entries are missing");

    for i in 0..assignments.len() {
        for j in (i + 1)..assignments.len() {
            let (a, room_a, start_a) = &assignments[i];
            let (b, room_b, start_b) = &assignments[j];
            if !intervals_overlap(a, *start_a, b, *start_b) {
                continue;
            }

            assert!(
                !a.blocker && !b.blocker,
                "blocker overlap: {:?} and {:?}",
                a.id,
                b.id
            );
            assert_ne!(
                room_a, room_b,
                "room overlap: {:?} and {:?} both use {:?}",
                a.id, b.id, room_a
            );
            assert!(
                a.instructor_extension_uuids
                    .iter()
                    .all(|instructor| !b.instructor_extension_uuids.contains(instructor)),
                "required instructor overlap: {:?} and {:?}",
                a.id,
                b.id
            );
        }
    }
}

/// Busy intervals are half-open: [start - preparation, start + duration + follow-up).
fn intervals_overlap(a: &FractivityEntry, start_a: i32, b: &FractivityEntry, start_b: i32) -> bool {
    let a_begin = i64::from(start_a) - i64::from(a.preparation_time);
    let a_end = i64::from(start_a) + i64::from(a.duration) + i64::from(a.follow_up_time);
    let b_begin = i64::from(start_b) - i64::from(b.preparation_time);
    let b_end = i64::from(start_b) + i64::from(b.duration) + i64::from(b.follow_up_time);

    a_begin < b_end && b_begin < a_end
}

#[test]
fn empty_input_returns_empty_distribution() {
    let result = solve(&[], vec![]).expect("empty input should be feasible");
    assert!(result.is_empty());
}

#[test]
fn one_activity_is_assigned_to_an_allowed_room_and_start_range() {
    let entries = vec![unpinned(entry(
        1,
        &[10],
        &[],
        30,
        &[100],
        &[0, 60],
        5,
        5,
        false,
    ))];

    let result = solve(&entries, vec![room(100, 0)]).expect("single activity should be feasible");
    assert_valid_distribution(&entries, &result);
}

#[test]
fn empty_room_or_start_list_is_reported_as_invalid_input() {
    let no_rooms = vec![unpinned(entry(1, &[], &[], 30, &[], &[0, 30], 0, 0, false))];
    let failures = solve(&no_rooms, vec![room(100, 0)]).unwrap_err();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].0.id, uid(1));
    assert_eq!(
        failures[0].1,
        FractivityDistributionFailReason::NoRoomsOrStarts
    );

    let no_starts = vec![unpinned(entry(2, &[], &[], 30, &[100], &[], 0, 0, false))];
    let failures = solve(&no_starts, vec![room(100, 0)]).unwrap_err();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].0.id, uid(2));
    assert_eq!(
        failures[0].1,
        FractivityDistributionFailReason::NoRoomsOrStarts
    );
}

#[test]
fn pinned_room_and_start_are_preserved() {
    let entries = vec![pinned(
        entry(1, &[], &[], 20, &[100, 200], &[0, 60], 5, 5, false),
        200,
        30,
    )];
    let result = solve(&entries, vec![room(100, 0), room(200, 1)])
        .expect("a valid pinned assignment should be feasible");

    assert_valid_distribution(&entries, &result);
    assert_eq!(result[0].1, uid(200));
    assert_eq!(result[0].2, 30);
}

#[test]
fn two_activities_can_share_a_room_when_scheduled_sequentially() {
    let entries = vec![
        unpinned(entry(1, &[10], &[], 30, &[100], &[0, 60], 0, 0, false)),
        unpinned(entry(2, &[20], &[], 30, &[100], &[0, 60], 0, 0, false)),
    ];
    let result = solve(&entries, vec![room(100, 0)])
        .expect("activities with a feasible sequential schedule should be distributed");

    assert_valid_distribution(&entries, &result);
    assert_eq!(result[0].1, result[1].1);
    assert!(!intervals_overlap(
        &result[0].0,
        result[0].2,
        &result[1].0,
        result[1].2
    ));
}

#[test]
fn activities_with_a_shared_required_instructor_are_not_overlapping() {
    let entries = vec![
        unpinned(entry(1, &[10], &[], 30, &[100], &[0, 60], 0, 0, false)),
        unpinned(entry(2, &[10], &[], 30, &[200], &[0, 60], 0, 0, false)),
    ];
    let result = solve(&entries, vec![room(100, 0), room(200, 0)])
        .expect("a sequential schedule should cover the shared instructor");

    assert_valid_distribution(&entries, &result);
    assert!(!intervals_overlap(
        &result[0].0,
        result[0].2,
        &result[1].0,
        result[1].2
    ));
}

#[test]
fn blocker_must_not_overlap_another_activity_even_in_a_different_room() {
    let entries = vec![
        unpinned(entry(1, &[], &[], 30, &[100], &[0, 60], 0, 0, true)),
        unpinned(entry(2, &[], &[], 30, &[200], &[0, 60], 0, 0, false)),
    ];
    let result = solve(&entries, vec![room(100, 0), room(200, 0)])
        .expect("the entries can be scheduled sequentially");

    assert_valid_distribution(&entries, &result);
    assert!(!intervals_overlap(
        &result[0].0,
        result[0].2,
        &result[1].0,
        result[1].2
    ));
}

#[test]
fn touching_busy_intervals_may_use_the_same_room() {
    let entries = vec![
        pinned(entry(1, &[], &[], 30, &[100], &[0, 60], 0, 0, false), 100, 0),
        pinned(entry(2, &[], &[], 30, &[100], &[0, 60], 0, 0, false), 100, 30),
    ];
    let result = solve(&entries, vec![room(100, 0)])
        .expect("half-open intervals that touch should not conflict");

    assert_valid_distribution(&entries, &result);
    assert_eq!(result[0].1, result[1].1);
}

#[test]
fn conflicting_pinned_activities_are_not_reported_as_a_successful_schedule() {
    let entries = vec![
        pinned(entry(1, &[], &[], 30, &[100], &[0, 60], 0, 0, false), 100, 15),
        pinned(entry(2, &[], &[], 30, &[100], &[0, 60], 0, 0, false), 100, 15),
    ];

    assert!(
        solve(&entries, vec![room(100, 0)]).is_err(),
        "two overlapping activities pinned to the same room cannot both be assigned"
    );
}

#[test]
fn same_time_and_same_required_instructor_cannot_be_fully_covered() {
    let entries = vec![
        pinned(entry(1, &[10], &[], 30, &[100], &[0, 60], 0, 0, false), 100, 15),
        pinned(entry(2, &[10], &[], 30, &[200], &[0, 60], 0, 0, false), 200, 15),
    ];

    let failures = solve(&entries, vec![room(100, 0), room(200, 0)]).unwrap_err();
    assert!(
        failures.iter().any(|(_, reason)| matches!(
            reason,
            FractivityDistributionFailReason::InstructorUnavailable(instructors)
                if instructors.contains(&uid(10))
        )),
        "the result should explain that the required instructor is unavailable"
    );
}

#[test]
fn room_priority_improvement_prefers_the_highest_feasible_priority() {
    let entries = vec![unpinned(entry(1, &[], &[], 30, &[100, 200], &[0, 60], 0, 0, false))];
    let result = solve(&entries, vec![room(100, 1), room(200, 9)])
        .expect("the activity can use either room");

    assert_valid_distribution(&entries, &result);
    assert_eq!(result[0].1, uid(200));
}

// ---------------------------------------------------------------------------
// Tiny exhaustive oracle
// ---------------------------------------------------------------------------
//
// The production model uses continuous start variables and rounds the result.
// For these tiny test cases, enumerating every integer start between the first
// and last allowed start is sufficient: all durations/preparation/follow-up
// values are integers, and rounding a feasible interval schedule preserves the
// ordering inequalities. This deliberately mirrors the current API's
// first_start..=last_start interval semantics, not a discrete-choice
// interpretation of every member of allowed_starts.
//
// These oracle fixtures avoid duplicate IDs and activities that list the same
// person as both an instructor and a visitor. Visitors are optional in the
// model; the oracle computes the maximum number that can be covered for each
// candidate schedule. It uses no MILP variables or reference-solver code.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OracleChoice {
    room: Uuid,
    start: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct OracleScore {
    // Field order is the lexicographic optimization priority.
    distributed: usize,
    required_instructor_coverage: usize,
    visitor_coverage: usize,
    sequential_pairs: usize,
}

#[derive(Debug)]
struct OracleSummary {
    feasible_schedules: usize,
    best_score: Option<OracleScore>,
    best_choices: Vec<OracleChoice>,
}

fn oracle_choices_for_entry(
    input: &InputEntry,
    rooms: &[FractivityRoom],
) -> Vec<OracleChoice> {
    let (activity, pin) = input;
    if activity.allowed_rooms.is_empty() || activity.allowed_starts.is_empty() {
        return Vec::new();
    }

    let first = activity.allowed_starts[0];
    let last = *activity.allowed_starts.last().unwrap();
    if first > last {
        return Vec::new();
    }

    // Keep the oracle intentionally small: it is for tiny unit-test instances,
    // not for the randomized performance benchmark.
    assert!(
        i64::from(last) - i64::from(first) <= 100,
        "brute-force oracle start range is too large for activity {:?}",
        activity.id
    );

    let room_ids: Vec<Uuid> = match pin {
        Some((pinned_room, _)) => {
            // A pinned assignment is expected to be a valid input assignment.
            if !activity.allowed_rooms.contains(pinned_room)
                || !rooms.iter().any(|r| r.id == *pinned_room)
            {
                return Vec::new();
            }
            vec![*pinned_room]
        }
        None => activity
            .allowed_rooms
            .iter()
            .copied()
            .filter(|room_id| rooms.iter().any(|r| r.id == *room_id))
            .collect(),
    };

    let starts: Vec<i32> = match pin {
        Some((_, pinned_start)) => {
            if *pinned_start < first || *pinned_start > last {
                return Vec::new();
            }
            vec![*pinned_start]
        }
        None => (first..=last).collect(),
    };

    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for room_id in room_ids {
        for &start in &starts {
            let choice = OracleChoice {
                room: room_id,
                start,
            };
            if seen.insert((choice.room, choice.start)) {
                result.push(choice);
            }
        }
    }
    result
}

fn oracle_pair_is_feasible(
    a: &FractivityEntry,
    choice_a: OracleChoice,
    b: &FractivityEntry,
    choice_b: OracleChoice,
) -> bool {
    if !intervals_overlap(a, choice_a.start, b, choice_b.start) {
        return true;
    }

    if a.blocker || b.blocker {
        return false;
    }
    if choice_a.room == choice_b.room {
        return false;
    }
    if a
        .instructor_extension_uuids
        .iter()
        .any(|id| b.instructor_extension_uuids.contains(id))
    {
        return false;
    }

    // Visitors are optional; their maximum possible coverage is scored below,
    // rather than making every listed visitor a hard feasibility condition.
    true
}

fn maximum_visitor_coverage(
    entries: &[InputEntry],
    choices: &[OracleChoice],
) -> usize {
    let visitor_ids: HashSet<Uuid> = entries
        .iter()
        .flat_map(|(activity, _)| activity.visitor_extension_uuids.iter().copied())
        .collect();

    let mut coverage = 0;
    for visitor_id in visitor_ids {
        // Required-instructor appearances of this same person are mandatory
        // uses, so an optional visitor assignment cannot overlap one.
        let required_intervals: Vec<(i64, i64)> = entries
            .iter()
            .enumerate()
            .filter(|(_, (activity, _))| {
                activity.instructor_extension_uuids.contains(&visitor_id)
            })
            .map(|(index, (activity, _))| {
                busy_interval(activity, choices[index].start)
            })
            .collect();

        let mut visitor_intervals: Vec<(i64, i64)> = entries
            .iter()
            .enumerate()
            .filter(|(_, (activity, _))| {
                activity.visitor_extension_uuids.contains(&visitor_id)
            })
            .map(|(index, (activity, _))| {
                busy_interval(activity, choices[index].start)
            })
            .filter(|candidate| {
                !required_intervals
                    .iter()
                    .any(|required| interval_tuples_overlap(*candidate, *required))
            })
            .collect();

        // Maximum-cardinality non-overlapping subset of intervals: the
        // classic earliest-finish-time greedy algorithm.
        visitor_intervals.sort_by_key(|(start, end)| (*end, *start));
        let mut last_end: Option<i64> = None;
        for (start, end) in visitor_intervals {
            if last_end.map_or(true, |previous_end| start >= previous_end) {
                coverage += 1;
                last_end = Some(end);
            }
        }
    }
    coverage
}

fn busy_interval(activity: &FractivityEntry, start: i32) -> (i64, i64) {
    (
        i64::from(start) - i64::from(activity.preparation_time),
        i64::from(start)
            + i64::from(activity.duration)
            + i64::from(activity.follow_up_time),
    )
}

fn interval_tuples_overlap(a: (i64, i64), b: (i64, i64)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

fn oracle_score(entries: &[InputEntry], choices: &[OracleChoice]) -> OracleScore {
    debug_assert_eq!(entries.len(), choices.len());

    let mut sequential_pairs = 0;
    for a in 0..entries.len() {
        for b in (a + 1)..entries.len() {
            if !intervals_overlap(
                &entries[a].0,
                choices[a].start,
                &entries[b].0,
                choices[b].start,
            ) {
                sequential_pairs += 1;
            }
        }
    }

    OracleScore {
        distributed: entries.len(),
        required_instructor_coverage: entries
            .iter()
            .map(|(activity, _)| activity.instructor_extension_uuids.len())
            .sum(),
        visitor_coverage: maximum_visitor_coverage(entries, choices),
        // The MILP omits pair variables for pairs that are always sequential.
        // Counting them here only adds a constant to every complete schedule,
        // so it does not change which schedule is optimal.
        sequential_pairs,
    }
}

fn enumerate_oracle_schedules(
    entries: &[InputEntry],
    candidate_choices: &[Vec<OracleChoice>],
    next_entry: usize,
    current: &mut Vec<OracleChoice>,
    summary: &mut OracleSummary,
) {
    if next_entry == entries.len() {
        summary.feasible_schedules += 1;
        let score = oracle_score(entries, current);
        if summary.best_score.map_or(true, |best| score > best) {
            summary.best_score = Some(score);
            summary.best_choices.clone_from(current);
        }
        return;
    }

    for &candidate in &candidate_choices[next_entry] {
        let compatible_with_previous = (0..next_entry).all(|previous| {
            oracle_pair_is_feasible(
                &entries[previous].0,
                current[previous],
                &entries[next_entry].0,
                candidate,
            )
        });
        if !compatible_with_previous {
            continue;
        }

        current.push(candidate);
        enumerate_oracle_schedules(
            entries,
            candidate_choices,
            next_entry + 1,
            current,
            summary,
        );
        current.pop();
    }
}

fn brute_force_oracle(
    entries: &[InputEntry],
    rooms: &[FractivityRoom],
) -> OracleSummary {
    let candidate_choices: Vec<Vec<OracleChoice>> = entries
        .iter()
        .map(|input| oracle_choices_for_entry(input, rooms))
        .collect();

    let mut summary = OracleSummary {
        feasible_schedules: 0,
        best_score: None,
        best_choices: Vec::new(),
    };
    if candidate_choices.iter().any(Vec::is_empty) {
        return summary;
    }

    let mut current = Vec::with_capacity(entries.len());
    enumerate_oracle_schedules(
        entries,
        &candidate_choices,
        0,
        &mut current,
        &mut summary,
    );
    summary
}

fn score_solver_assignments(entries: &[InputEntry], assignments: &[Assignment]) -> OracleScore {
    let choices: Vec<OracleChoice> = entries
        .iter()
        .map(|(original, _)| {
            let (_, assigned_room, assigned_start) = assignments
                .iter()
                .find(|(returned, _, _)| returned.id == original.id)
                .unwrap_or_else(|| panic!("solver omitted activity {:?}", original.id));
            OracleChoice {
                room: *assigned_room,
                start: *assigned_start,
            }
        })
        .collect();
    oracle_score(entries, &choices)
}

/// Asserts feasibility when the exhaustive oracle finds a complete schedule,
/// and compares the solver's lexicographic score to the true optimum. If the
/// oracle finds no complete schedule, the public API must not report success.
fn assert_matches_brute_force_oracle(
    entries: &[InputEntry],
    rooms: Vec<FractivityRoom>,
) -> OracleSummary {
    let oracle = brute_force_oracle(entries, &rooms);
    let result = solve(entries, rooms);

    match (oracle.best_score, result) {
        (Some(expected), Ok(actual)) => {
            assert_valid_distribution(entries, &actual);
            let actual_score = score_solver_assignments(entries, &actual);
            assert_eq!(
                actual_score,
                expected,
                "solver score differs from exhaustive optimum; oracle enumerated {} complete schedules; one optimal witness was {:?}",
                oracle.feasible_schedules,
                oracle.best_choices,
            );
        }
        (Some(expected), Err(failures)) => {
            panic!(
                "oracle found a feasible schedule with score {:?}, but distribute reported failures: {:?}",
                expected,
                failures.iter().map(|(activity, reason)| (activity.id, reason)).collect::<Vec<_>>(),
            );
        }
        (None, Ok(actual)) => {
            panic!(
                "solver reported success, but exhaustive enumeration found no feasible complete schedule: {:?}",
                actual.iter().map(|(activity, room, start)| (activity.id, *room, *start)).collect::<Vec<_>>(),
            );
        }
        (None, Err(_)) => {}
    }

    oracle
}

#[test]
fn brute_force_oracle_checks_optimal_pairwise_separation() {
    // Each activity has its own room, so all three can be distributed. The
    // 10-minute busy intervals must fit into a 0..=10 start window. At most
    // two of the three activity pairs can be sequential; the third pair must
    // overlap. This makes the oracle check the optimization objective, not
    // just schedule feasibility.
    let entries = vec![
        unpinned(entry(1, &[], &[], 10, &[100], &[0, 10], 0, 0, false)),
        unpinned(entry(2, &[], &[], 10, &[200], &[0, 10], 0, 0, false)),
        unpinned(entry(3, &[], &[], 10, &[300], &[0, 10], 0, 0, false)),
    ];
    let rooms = vec![room(100, 0), room(200, 0), room(300, 0)];

    let oracle = assert_matches_brute_force_oracle(&entries, rooms);
    let best = oracle.best_score.expect("this instance is feasible");
    assert_eq!(best.distributed, 3);
    assert_eq!(best.visitor_coverage, 0);
    assert_eq!(best.sequential_pairs, 2);
}

#[test]
fn brute_force_oracle_checks_optional_visitor_coverage_before_pair_score() {
    // The same visitor is available for all three activities. At most two
    // can receive that visitor because their busy intervals cannot all be
    // pairwise disjoint in this short window. The solver should first maximize
    // visitor coverage and then maximize sequential pairs among schedules
    // attaining that coverage.
    let entries = vec![
        unpinned(entry(11, &[], &[70], 10, &[110], &[0, 10], 0, 0, false)),
        unpinned(entry(12, &[], &[70], 10, &[120], &[0, 10], 0, 0, false)),
        unpinned(entry(13, &[], &[70], 10, &[130], &[0, 10], 0, 0, false)),
    ];
    let rooms = vec![room(110, 0), room(120, 0), room(130, 0)];

    let oracle = assert_matches_brute_force_oracle(&entries, rooms);
    let best = oracle.best_score.expect("this instance is feasible");
    assert_eq!(best.visitor_coverage, 2);
    assert_eq!(best.sequential_pairs, 2);
}

#[test]
fn brute_force_oracle_accounts_for_required_instructor_conflicts() {
    let entries = vec![
        unpinned(entry(21, &[50], &[], 10, &[210], &[0, 10], 0, 0, false)),
        unpinned(entry(22, &[50], &[], 10, &[220], &[0, 10], 0, 0, false)),
        unpinned(entry(23, &[], &[], 10, &[230], &[0, 10], 0, 0, false)),
    ];
    let rooms = vec![room(210, 0), room(220, 0), room(230, 0)];

    let oracle = assert_matches_brute_force_oracle(&entries, rooms);
    let best = oracle.best_score.expect("this instance is feasible");
    assert_eq!(best.required_instructor_coverage, 2);
    assert_eq!(best.sequential_pairs, 2);
}

#[test]
fn brute_force_oracle_rejects_a_pinned_room_conflict() {
    let entries = vec![
        pinned(entry(31, &[], &[], 10, &[310], &[0, 10], 0, 0, false), 310, 5),
        pinned(entry(32, &[], &[], 10, &[310], &[0, 10], 0, 0, false), 310, 5),
    ];
    let rooms = vec![room(310, 0)];
    let oracle = brute_force_oracle(&entries, &rooms);
    assert!(oracle.best_score.is_none(), "oracle should find no feasible schedule");
    let result = solve(&entries, rooms);
    assert!(result.is_err(), "solver must not report a successful distribution");
}