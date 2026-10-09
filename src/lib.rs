use std::collections::{HashMap, HashSet};
#[cfg(test)]
mod distribution_tests;

mod distribution_checks;

use uuid::Uuid;

use good_lp::{
    constraint,
    solvers::highs::{self, HighsProblem},
    variable, variables, Expression, IntoAffineExpression, ObjectiveDirection, ProblemVariables,
    Solution, SolverModel, Variable,
};
// Fields that should not matter are intentionally removed
#[derive(Clone, Debug)]
pub struct FractivityRoom {
    pub id: Uuid,
    pub priority: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FractivityEntry {
    pub id: Uuid,
    pub topic: String,
    pub instructor_extension_uuids: Vec<Uuid>,
    pub visitor_extension_uuids: Vec<Uuid>,
    pub duration: i32,
    pub allowed_rooms: Vec<Uuid>,
    pub allowed_starts: Vec<i32>,
    pub preparation_time: i32,
    pub follow_up_time: i32,
    pub blocker: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DistributionFractivityEntry {
    pub id: Uuid,
    pub instructor_visitor_indices: Vec<(usize, bool)>,
    pub duration: i32,
    pub allowed_rooms: Vec<usize>,
    pub first_start: i32,
    pub last_start: i32,
    pub busy_start: i32,
    pub busy_end: i32,
    pub blocker: bool,
}

struct FractivityDistributionVars {
    distributed: Variable,
    start: Variable,
    room: Vec<Variable>,
    instructors: Vec<Variable>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FractivityDistributionFailReason {
    InstructorUnavailable(Vec<Uuid>),
    RoomUnavailable,
    NoRoomsOrStarts,
}

struct ModelVars {
    fractivities: Vec<FractivityDistributionVars>,
    pairs: Vec<ConflictingPairInformation>,
}

struct ConflictingPairInformation {
    a: usize,
    b: usize,
    overlapping_instructors: Vec<(usize,usize)>,
    overlapping_rooms: Vec<(usize,usize)>,
    ordered_ab: Variable,
    ordered_ba: Variable,
}

pub fn distribute(
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
    all_rooms: Vec<FractivityRoom>,
) -> Result<
    Result<
        Vec<(FractivityEntry, Uuid, i32)>,
        Vec<(FractivityEntry, FractivityDistributionFailReason)>,
    >,
    String,
> {
    if fractivity_entries.is_empty() {
        return Ok(Ok(Vec::new()));
    }
    let mut instructors_aggregator = HashSet::new();
    let mut rooms_aggregator = HashSet::new();

    println!("Distributing the following entries:");

    for entry in fractivity_entries {
        println!(
            "{} -- starts: {:?} rooms: {:?} preparation: {:?} duration: {:?} follow-up: {:?}",
            entry.0.topic,
            entry.0.allowed_starts,
            entry.0.allowed_rooms,
            entry.0.preparation_time,
            entry.0.duration,
            entry.0.follow_up_time
        );
        if entry.0.allowed_rooms.is_empty() || entry.0.allowed_starts.is_empty() {
            return Ok(Err(vec![(
                entry.0.clone(),
                FractivityDistributionFailReason::NoRoomsOrStarts,
            )]));
        }
        for room in &entry.0.allowed_rooms {
            rooms_aggregator.insert(*room);
        }
        for instructor in &entry.0.instructor_extension_uuids {
            instructors_aggregator.insert(*instructor);
        }
        for instructor in &entry.0.visitor_extension_uuids {
            instructors_aggregator.insert(*instructor);
        }
    }

    let all_instructors: Vec<Uuid> = instructors_aggregator.into_iter().collect();

    println!("All instructors: {:?}", all_instructors);

    // TODO technically we only need from one event here. Makes this more efficient
    /*let room_meta: HashMap<Uuid, FractivityRoom> = match FractivityRoom::find_all() {
        Ok(v) => v,
        Err(_) => Vec::new(),
    }
    .into_iter()
    .map(|room| (room.id, room))
    .collect();
    let all_rooms: Vec<FractivityRoom> = rooms_aggregator
        .into_iter()
        .map(|room| match room_meta.get(&room) {
            Some(r) => Ok(r.clone()),
            None => Err(format!("Room with uuid {} not found overall", room)),
        })
        .collect::<Result<Vec<FractivityRoom>,String>>()?;*/

    let mut converted_fractivities = Vec::new();
    for (entry,distribution) in fractivity_entries {
        let mut instructor_visitor_indices = Vec::new();
        for instructor in &entry.instructor_extension_uuids {
            match all_instructors.iter().position(|i| i == instructor) {
                Some(i) => instructor_visitor_indices.push((i,false)),
                None => (),
            }
        }
        for visitor in &entry.visitor_extension_uuids {
            match all_instructors.iter().position(|i| i == visitor) {
                Some(i) => instructor_visitor_indices.push((i,true)),
                None => (),
            }
        }
        instructor_visitor_indices.sort_by_key(|(i,_)| *i);

        let mut room_indices = Vec::new();
        for room in &entry.allowed_rooms {
            match all_rooms.iter().position(|r| r.id == *room) {
                Some(i) => room_indices.push(i),
                None => (),
            }
        }
        let mut first_start = *entry.allowed_starts.first().expect("Emptyness of allowed starts was checked above") - entry.preparation_time;
        let mut last_start = *entry.allowed_starts.last().expect("Emptyness of allowed starts was checked above") - entry.preparation_time;

        match distribution {
            Some((room, time)) => {
                room_indices.clear();
                match all_rooms.iter().position(|r| r.id == *room) {
                    Some(i) => room_indices.push(i),
                    None => (),
                }
                first_start = *time - entry.preparation_time;
                last_start = *time - entry.preparation_time;
            }
            None => ()
        }

        converted_fractivities.push(DistributionFractivityEntry{
            id: entry.id, 
            instructor_visitor_indices,
            duration: entry.duration + entry.follow_up_time + entry.preparation_time,
            allowed_rooms: room_indices,
            first_start,
            last_start,
            blocker: entry.blocker,
            busy_start: first_start,
            busy_end: last_start + entry.duration + entry.follow_up_time + entry.preparation_time,
        })
    }

    Ok(
        lp_distribution(&converted_fractivities, &all_rooms, &all_instructors)?
            .map_err(|v| {
                v.iter()
                    .map(|(entry_idx, reason)| {
                        (fractivity_entries[*entry_idx].0.clone(), reason.clone())
                    })
                    .collect()
            })
            .map(|v| {
                v.iter()
                    .map(|(entry_idx, room, s)| {
                        (fractivity_entries[*entry_idx].0.clone(), *room, *s + fractivity_entries[*entry_idx].0.preparation_time)
                    })
                    .collect()
            }),
    )
}

fn create_fractivity_variables(
    fractivity_entries: &[DistributionFractivityEntry],
    all_vars: &mut ProblemVariables,
) -> Vec<FractivityDistributionVars>
 {
    let mut fractivity_variables = Vec::new();

    for a in fractivity_entries {
        let distributed = all_vars.add(variable().binary());
        let start = all_vars.add(variable().clamp(a.first_start, a.last_start));

        let mut room = Vec::new();
        for _ in &a.allowed_rooms {
            room.push(all_vars.add(variable().binary()));
        }

        let mut instructors = Vec::new();
        for _ in &a.instructor_visitor_indices {
            instructors.push(all_vars.add(variable().binary()));
        }
        fractivity_variables.push(FractivityDistributionVars{
            distributed,
            start,
            room,
            instructors
        })
    }

    fractivity_variables
}

fn create_fractivity_pair_variables(
    fractivity_entries: &[DistributionFractivityEntry],
    all_vars: &mut ProblemVariables,
) -> Vec<ConflictingPairInformation>
 {
    let mut pair_information = Vec::new();
    for a in 0..fractivity_entries.len() {
        for b in (a + 1)..fractivity_entries.len() {
            if fractivity_entries[a].busy_end <= fractivity_entries[b].busy_start
                || fractivity_entries[b].busy_end <= fractivity_entries[a].busy_start
            {
                continue;
            }
            let mut overlapping_rooms = Vec::new();
            for  (i,room) in fractivity_entries[a].allowed_rooms.iter().enumerate() {
                let i2 = fractivity_entries[b].allowed_rooms.iter().position(|r| r == room);
                match i2 {
                    Some(index) => overlapping_rooms.push((i,index)),
                    None => ()
                }
            }
            let mut overlapping_instructors = Vec::new();
            for  (i,(instructor,_)) in fractivity_entries[a].instructor_visitor_indices.iter().enumerate() {
                let i2 = fractivity_entries[b].instructor_visitor_indices.iter().position(|(i,_)| i == instructor);
                match i2 {
                    Some(index) => overlapping_instructors.push((i,index)),
                    None => ()
                }
            }
            let ordered_ab = all_vars.add(variable().binary());
            let ordered_ba = all_vars.add(variable().binary());
            pair_information.push(ConflictingPairInformation {
                a,
                b,
                overlapping_rooms,
                overlapping_instructors,
                ordered_ab,
                ordered_ba
            });
        }
    }
     pair_information
}


fn add_constraints(
    fractivity_entries: &[DistributionFractivityEntry],
    problem: &mut HighsProblem,
    vars: &ModelVars,
) {
    // Distribute max once and only in legal rooms and starts
    for a in &vars.fractivities {
        problem.add_constraint(constraint!(a.room.iter().sum::<Expression>() == 1));
        
    }
    for pair in &vars.pairs {
        let a = pair.a;
        let b = pair.b;

        problem.add_constraint(constraint!(
            pair.ordered_ab + pair.ordered_ba <= 1
        ));

        let Mab = fractivity_entries[a].busy_end - fractivity_entries[b].busy_start;
        let Mba = fractivity_entries[b].busy_end - fractivity_entries[a].busy_start;

        problem.add_constraint(constraint!(
            vars.fractivities[a].start
                + fractivity_entries[a].duration
                <= vars.fractivities[b].start 
                    + Mab * (1 - pair.ordered_ab)
        ));
        problem.add_constraint(constraint!(
            vars.fractivities[b].start
                + fractivity_entries[b].duration
                <= vars.fractivities[a].start 
                    + Mba * (1 - pair.ordered_ba)
        ));

        if fractivity_entries[a].blocker || fractivity_entries[b].blocker {
            problem.add_constraint(constraint!(
                pair.ordered_ab
                    + pair.ordered_ba
                    + (1 - vars.fractivities[a].distributed)
                    + (1 - vars.fractivities[b].distributed)
                    >= 1
            ));
            continue;
        }

        for (ra,rb) in &pair.overlapping_rooms {
            problem.add_constraint(constraint!(
                vars.fractivities[a].room[*ra]
                    + vars.fractivities[b].room[*rb]
                    + vars.fractivities[a].distributed
                    + vars.fractivities[b].distributed
                    - pair.ordered_ab
                    - pair.ordered_ba
                    <= 3
            ));
            
        }

        for (ia,ib) in &pair.overlapping_instructors {
            problem.add_constraint(constraint!(
                vars.fractivities[a].instructors[*ia]
                    + vars.fractivities[b].instructors[*ib]
                    + vars.fractivities[a].distributed
                    + vars.fractivities[b].distributed
                    - pair.ordered_ab
                    - pair.ordered_ba
                    <= 3
            ));
            
        }
    }
}

fn solve_stage(
    fractivity_entries: &[DistributionFractivityEntry],
    all_vars: &ProblemVariables,
    vars: &ModelVars,
    objective: Expression,
    sense: ObjectiveDirection,
    parallel_total: f32,
) -> Result<impl Solution, String> {
    let mut problem = all_vars
        .clone()
        .optimise(sense, objective.clone())
        .using(highs::highs);
    problem = problem.set_mip_rel_gap(0.0).map_err(|e| e.to_string())?;
    problem = problem
        .set_mip_abs_gap(if parallel_total > 0.0  {parallel_total * 0.5} else {0.0})
        .map_err(|e| e.to_string())?;


    add_constraints(
        fractivity_entries,
        &mut problem,
        vars,
    );

    problem.solve().map_err(|e| e.to_string())
}

/// Two fractivities in the same room are compatible if one of them is completely
/// over (duration + follow-up) and the preparation of the later one has fit in
/// before it starts. This mirrors the big-M separation constraints in
/// `add_constraints`, including which fractivity's preparation time is used.
fn can_share_room(a: &DistributionFractivityEntry, a_start: i32, b: &DistributionFractivityEntry, b_start: i32) -> bool {
    let a_before_b = b_start >= a_start + a.duration;
    let b_before_a = a_start >= b_start + b.duration;
    a_before_b || b_before_a
}

/// Greedy post-processing: moves fractivities into better rooms as long as no
/// time conflict with a fractivity already in that room is created.
/// Starts and instructors are never touched, so the result stays feasible.
fn room_distribution_improvements(
    fractivity_entries: &[DistributionFractivityEntry],
    mut distribution: Vec<(usize, Uuid, i32)>,
    all_rooms: &[FractivityRoom],
) -> Vec<(usize, Uuid, i32)> {
    // Larger score = better room. The LP objective maximises the sum of room
    // priorities, so the score is the priority itself. Use `-r.priority` here
    // if rooms with a *lower* priority value should be preferred.
    let room_score: HashMap<Uuid, i32> = all_rooms.iter().map(|r| (r.id, r.priority)).collect();

    // Every move strictly increases the score of one fractivity, so this
    // terminates. Repeating is needed because a move frees up the old room.
    loop {
        let mut moved = false;

        for i in 0..distribution.len() {
            let (entry_idx, current_room, start) = distribution[i];
            let entry = &fractivity_entries[entry_idx];

            // Fractivities fixed to a room (the LP only offers that room) stay put
            if entry.allowed_rooms.len() <= 1 {
                continue;
            }
            let Some(&current_score) = room_score.get(&current_room) else {
                continue;
            };

            // Strictly better allowed rooms, best first
            let mut candidates: Vec<(usize, i32)> = entry
                .allowed_rooms
                .iter()
                .map(|room|  (*room, all_rooms[*room].priority))
                .filter(|(_, score)| *score > current_score)
                .collect();
            candidates.sort_by_key(|a| a.1);

            for (room, _) in candidates {
                let no_conflict = distribution.iter().enumerate().all(
                    |(j, (other_idx, other_room, other_start))| {
                        j == i
                            || *other_room != all_rooms[room].id
                            || can_share_room(
                                entry,
                                start,
                                &fractivity_entries[*other_idx],
                                *other_start,
                            )
                    },
                );
                if no_conflict {
                    distribution[i].1 = all_rooms[room].id;
                    moved = true;
                    break;
                }
            }
        }

        if !moved {
            break;
        }
    }

    distribution
}

fn lp_distribution(
    fractivity_entries: &[DistributionFractivityEntry],
    all_rooms: &[FractivityRoom],
    all_instructors: &[Uuid],
) -> Result<Result<Vec<(usize, Uuid, i32)>, Vec<(usize, FractivityDistributionFailReason)>>, String>
{
    let mut all_vars = variables!();


    let vars = ModelVars {
        fractivities: create_fractivity_variables(fractivity_entries, &mut all_vars),
        pairs: create_fractivity_pair_variables(fractivity_entries,&mut all_vars ),
    };

    let obj_distributed: Expression = vars.fractivities.iter().map(|f| &f.distributed).copied().sum();

    let obj_instructors: Expression = vars
        .fractivities
        .iter()
        .enumerate()
        .flat_map(|(a, row)| {
            row.instructors.iter().enumerate().filter_map(move |(i, v)| {
                if !fractivity_entries[a].instructor_visitor_indices[i].1 {
                    Some(v)
                } else {
                    None
                }
        })
            })
        .sum();

    let obj_visitors: Expression = vars
        .fractivities
        .iter()
        .enumerate()
        .flat_map(|(a, row)| {
            row.instructors.iter().enumerate().filter_map(move |(i, v)| {
                if fractivity_entries[a].instructor_visitor_indices[i].1 {
                    Some(v)
                } else {
                    None
                }
        })
            })
        .sum();

    let obj_ordered: Expression = vars.pairs.iter().flat_map(|p| vec![p.ordered_ab, p.ordered_ba]).sum();

    let mut total_objective = obj_distributed.clone();
    total_objective *= obj_instructors
        .clone()
        .linear_coefficients()
        .map(|(_, p)| p)
        .sum::<f64>()
        + 1.0;
    total_objective += obj_instructors.clone();
    total_objective *= obj_visitors
        .clone()
        .linear_coefficients()
        .map(|(_, p)| p)
        .sum::<f64>()
        + 1.0;
    total_objective += obj_visitors.clone();
    total_objective *= obj_ordered
        .clone()
        .linear_coefficients()
        .map(|(_, p)| p)
        .sum::<f64>()
        + 1.0;
    total_objective += obj_ordered.clone();
    let parallel_total = obj_ordered
        .clone()
        .linear_coefficients()
        .map(|(_, p)| p.abs())
        .sum::<f64>();

    println!(
        "total objective coefficients sum {:?}",
        total_objective
            .clone()
            .linear_coefficients()
            .map(|(_, p)| p)
            .sum::<f64>()
    );

    let solution = solve_stage(
        fractivity_entries,
        &all_vars,
        &vars,
        total_objective,
        ObjectiveDirection::Maximisation,
        parallel_total as f32,
    )?;

    println!("Found solution");
    let mut result_distributed = Vec::new();
    let mut err_undistributed = Vec::new();

    for (index, a) in fractivity_entries.iter().enumerate() {
        let mut room_dist = None;
        for (i, v) in vars.fractivities[index].room.iter().enumerate() {
            if  solution.value(*v).round() > 0.0
            {
                room_dist = Some(all_rooms[a.allowed_rooms[i]].id);
                break;
            }
        }
        let room = match room_dist {
            Some(r) => r,
            None => {
                err_undistributed.push((index, FractivityDistributionFailReason::RoomUnavailable));
                break;
            }
        };
        let mut undistributed_instructors = Vec::new();
        for (i, v) in vars.fractivities[index].instructors.iter().enumerate() {
            if  solution.value(*v).round() == 0.0 && !a.instructor_visitor_indices[i].1
            {
                undistributed_instructors.push(all_instructors[a.instructor_visitor_indices[i].0]);
            }
        }
        if solution.value(vars.fractivities[index].distributed).round() == 0.0 {
            err_undistributed.push((index, FractivityDistributionFailReason::RoomUnavailable));
        } else if !undistributed_instructors.is_empty() {
            err_undistributed.push((
                index,
                FractivityDistributionFailReason::InstructorUnavailable(undistributed_instructors),
            ));
        } else {
            result_distributed.push((
                index,
                room,
                solution.value(vars.fractivities[index].start).round() as i32,
            ));
        }
    }
    if err_undistributed.is_empty() {
        Ok(Ok(room_distribution_improvements(
            fractivity_entries,
            result_distributed,
            all_rooms,
        )))
    } else {
        Ok(Err(err_undistributed))
    }
}
