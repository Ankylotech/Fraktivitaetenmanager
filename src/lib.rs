use std::{
    collections::{HashMap, HashSet},
};

mod distribution_checks;

use chrono::NaiveDate;
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
    pub start_day: NaiveDate,
    pub blocker: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FractivityDistributionFailReason {
    InstructorUnavailable(Vec<Uuid>),
    RoomUnavailable,
}

struct ModelVars {
    starts: Vec<Variable>,
    rooms: Vec<Vec<Option<Variable>>>,
    distributed: Vec<Variable>,
    instructors: Vec<Vec<Option<Variable>>>, // your `v[a][i]`, split by is_instructor/is_visitor when building objectives
    ordered_ab: Vec<Variable>,
    ordered_ba: Vec<Variable>
}

struct FractivityStartInformation {
    busy_start: i32,
    busy_end: i32,
}


struct ConflictingPairInformation {
    a: usize,
    b: usize,
    overlapping_instructors: Vec<usize>,
    overlapping_rooms: Vec<usize>
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
                FractivityDistributionFailReason::RoomUnavailable,
            )]));
        }
        for room in &entry.0.allowed_rooms {
            rooms_aggregator.insert(room.clone());
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

    Ok(
        lp_distribution(fractivity_entries, &all_rooms, &all_instructors)?.map(|v| {
            v.iter()
                .map(|(entry_idx, room, s)| (fractivity_entries[*entry_idx].0.clone(), *room, *s))
                .collect()
        }),
    )
}

fn create_fractivity_variables(
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
    all_rooms: &Vec<FractivityRoom>,
    all_vars: &mut ProblemVariables,
) -> (Vec<Variable>, Vec<Vec<Option<Variable>>>, Vec<Variable>, Vec<FractivityStartInformation>) {
    let mut starts = Vec::new();
    let mut rooms = Vec::new();
    let mut distributed = Vec::new();
    let mut start_information = Vec::new();

    for a in fractivity_entries {
        let mut first_start = a.0.allowed_starts[0];
        let mut last_start = *a.0.allowed_starts.last().unwrap();
        let d = all_vars.add(variable().binary());
        distributed.push(d);

        let mut room = Vec::new();
        for r in all_rooms {
            let mut res = None;
            match a.1 {
                Some((id, _)) => {
                    if id == r.id {
                        let v = all_vars.add(variable().binary());
                        res = Some(v);
                    }
                }
                None => {
                    if a.0.allowed_rooms.contains(&r.id) {
                        let v = all_vars.add(variable().binary());
                        res = Some(v);
                    }
                }
            }

            room.push(res);
        }
        match a.1 {
            Some((_, start)) => {
                first_start = start;
                last_start = start;
            }
            None => {}
        }
        let v = all_vars.add(variable().integer().clamp(first_start,last_start));
        starts.push(v);
        rooms.push(room);
        start_information.push(FractivityStartInformation{ busy_start: first_start - a.0.preparation_time, busy_end: last_start + a.0.duration + a.0.follow_up_time});
    }

    (starts, rooms, distributed, start_information)
}

fn create_fractivity_pair_variables(
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
    all_instructors: &Vec<Uuid>,
    all_rooms: &Vec<FractivityRoom>,
    all_vars: &mut ProblemVariables,
    start_information: &Vec<FractivityStartInformation>
) -> (Vec<Variable>, Vec<Variable>, Vec<ConflictingPairInformation>) {
    let mut ordered_ab = Vec::new();
    let mut ordered_ba = Vec::new();
    let mut pair_information = Vec::new();
    for a in 0..fractivity_entries.len() {
        for b in (a + 1)..fractivity_entries.len() {
            if start_information[a].busy_end <= start_information[b].busy_start || start_information[b].busy_end <= start_information[a].busy_start {
                continue;
            } 
            let mut overlapping_rooms = Vec::new();
            for (i,room) in all_rooms.iter().enumerate() {
                if  fractivity_entries[a].0.allowed_rooms.contains(&room.id) && fractivity_entries[b].0.allowed_rooms.contains(&room.id)  {
                    overlapping_rooms.push(i);
                }
            }
            let mut overlapping_instructors = Vec::new();
            for (i,instructor) in all_instructors.iter().enumerate() {
                if (fractivity_entries[a].0.instructor_extension_uuids.contains(instructor) || fractivity_entries[a].0.visitor_extension_uuids.contains(instructor)) && (fractivity_entries[b].0.instructor_extension_uuids.contains(instructor) || fractivity_entries[b].0.visitor_extension_uuids.contains(instructor)) {
                    overlapping_instructors.push(i);
                }
            }
            if overlapping_rooms.is_empty() && overlapping_instructors.is_empty()  {
                continue;
            }
            let ab = all_vars.add(variable().binary());
            ordered_ab.push(ab);
            let ba = all_vars.add(variable().binary());
            ordered_ba.push(ba);
            pair_information.push(ConflictingPairInformation { a,b, overlapping_rooms, overlapping_instructors});
        }
    }
    (
        ordered_ab,
        ordered_ba,
        pair_information,
    )
}

fn create_instructor_variables(
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
    all_instructors: &Vec<Uuid>,
    all_vars: &mut ProblemVariables,
) -> Vec<Vec<Option<Variable>>> {
    let mut v = Vec::new();

    for a in 0..fractivity_entries.len() {
        let mut va = Vec::new();
        for i in all_instructors {
            let mut res = None;
            if fractivity_entries[a]
                .0
                .instructor_extension_uuids
                .contains(&i)
            {
                let var = all_vars.add(variable().binary());
                res = Some(var);
            } else if fractivity_entries[a].0.visitor_extension_uuids.contains(&i) {
                let var = all_vars.add(variable().binary());
                res = Some(var);
            }

            va.push(res);
        }
        v.push(va);
    }
    v
}


fn add_constraints(
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
    problem: &mut HighsProblem,
    vars: &ModelVars,
    start_information: &Vec<FractivityStartInformation>,
    pair_information: &Vec<ConflictingPairInformation>
) {
    // Distribute max once and only in legal rooms and starts
    for a in 0..fractivity_entries.len() {

        let mut room_constr = Expression::default();
        for r in &vars.rooms[a] {
            if r.is_some() {
                room_constr.add_mul(1.0, r.unwrap());
            }
        }
        problem.add_constraint(constraint!(room_constr.clone() == 1));
    }
    for (index, pair) in pair_information.iter().enumerate() {

            let a = pair.a;
            let b = pair.b;

            problem.add_constraint(constraint!(
                    vars.ordered_ab[index] + vars.ordered_ba[index] <= 1
                ));

            let Mab  = start_information[a].busy_end - start_information[b].busy_start;
            let Mba  = start_information[b].busy_end - start_information[a].busy_start;

            problem.add_constraint(constraint!(vars.starts[a] + fractivity_entries[a].0.duration + fractivity_entries[a].0.follow_up_time <= vars.starts[b] - fractivity_entries[b].0.preparation_time + Mab*(1 - vars.ordered_ab[index])));
            problem.add_constraint(constraint!(vars.starts[b] + fractivity_entries[b].0.duration + fractivity_entries[b].0.follow_up_time <= vars.starts[a] - fractivity_entries[a].0.preparation_time + Mba*(1 - vars.ordered_ba[index])));

            if fractivity_entries[a].0.blocker || fractivity_entries[b].0.blocker {
                problem.add_constraint(constraint!(
                    vars.ordered_ab[index] + vars.ordered_ba[index] + (1 - vars.distributed[a]) + (1 - vars.distributed[b]) >= 1
                ));
                continue;
            }

            for r in &pair.overlapping_rooms {
                if vars.rooms[a][*r].is_some() && vars.rooms[b][*r].is_some() {
                    problem.add_constraint(constraint!(
                        vars.rooms[a][*r].unwrap() + vars.rooms[b][*r].unwrap() + vars.distributed[a] + vars.distributed[b] - vars.ordered_ab[index] - vars.ordered_ba[index] <= 3
                    ));
                }
            }

            for i in &pair.overlapping_instructors {
                if vars.instructors[a][*i].is_some() && vars.instructors[b][*i].is_some() {
                    problem.add_constraint(constraint!(vars.instructors[a][*i].unwrap() + vars.instructors[b][*i].unwrap() + vars.distributed[a] + vars.distributed[b] - vars.ordered_ab[index] - vars.ordered_ba[index] <= 3));
                }
            }
        
    }
}

fn solve_stage(
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
    all_vars: &ProblemVariables,
    vars: &ModelVars,
    objective: Expression,
    sense: ObjectiveDirection,
    parallel_total: f32,
    start_information: &Vec<FractivityStartInformation>,
    pair_information: &Vec<ConflictingPairInformation>
) -> Result<impl Solution, String> {
    let mut problem = all_vars
        .clone()
        .optimise(sense, objective.clone())
        .using(highs::highs);
    problem = problem.set_mip_rel_gap(0.0).map_err(|e| e.to_string())?;
    problem = problem
        .set_mip_abs_gap(parallel_total * 0.5)
        .map_err(|e| e.to_string())?;

    problem.set_verbose(true);

    add_constraints(
        fractivity_entries,
        &mut problem,
        vars,
        start_information,
        pair_information
    );

    problem.solve().map_err(|e| e.to_string())
}

/// Two fractivities in the same room are compatible if one of them is completely
/// over (duration + follow-up) and the preparation of the later one has fit in
/// before it starts. This mirrors the big-M separation constraints in
/// `add_constraints`, including which fractivity's preparation time is used.
fn can_share_room(a: &FractivityEntry, a_start: i32, b: &FractivityEntry, b_start: i32) -> bool {
    let a_before_b = b_start >= a_start + a.duration + a.follow_up_time + b.preparation_time;
    let b_before_a = a_start >= b_start + b.duration + b.follow_up_time + a.preparation_time;
    a_before_b || b_before_a
}

/// Greedy post-processing: moves fractivities into better rooms as long as no
/// time conflict with a fractivity already in that room is created.
/// Starts and instructors are never touched, so the result stays feasible.
fn room_distribution_improvements(
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
    mut distribution: Vec<(usize, Uuid, i32)>,
    all_rooms: &Vec<FractivityRoom>,
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
            let (entry, pinned) = &fractivity_entries[entry_idx];

            // Fractivities fixed to a room (the LP only offers that room) stay put
            if pinned.is_some() {
                continue;
            }
            let Some(&current_score) = room_score.get(&current_room) else {
                continue;
            };

            // Strictly better allowed rooms, best first
            let mut candidates: Vec<(Uuid, i32)> = entry
                .allowed_rooms
                .iter()
                .filter_map(|room| room_score.get(room).map(|score| (*room, *score)))
                .filter(|(_, score)| *score > current_score)
                .collect();
            candidates.sort_by(|a, b| b.1.cmp(&a.1));

            for (room, _) in candidates {
                let no_conflict = distribution.iter().enumerate().all(
                    |(j, (other_idx, other_room, other_start))| {
                        j == i
                            || *other_room != room
                            || can_share_room(
                                entry,
                                start,
                                &fractivity_entries[*other_idx].0,
                                *other_start,
                            )
                    },
                );
                if no_conflict {
                    distribution[i].1 = room;
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
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
    all_rooms: &Vec<FractivityRoom>,
    all_instructors: &Vec<Uuid>,
) -> Result<
    Result<Vec<(usize, Uuid, i32)>, Vec<(FractivityEntry, FractivityDistributionFailReason)>>,
    String,
> {
    let mut all_vars = variables!();
    let instructors =
        create_instructor_variables(fractivity_entries, all_instructors, &mut all_vars);
    let (starts, rooms, distributed, start_information) =
        create_fractivity_variables(fractivity_entries, all_rooms, &mut all_vars);
    let (ordered_ab, ordered_ba, pair_information) =
        create_fractivity_pair_variables(fractivity_entries, all_instructors, all_rooms,&mut all_vars, &start_information);

    let vars = ModelVars {
        starts,
        rooms,
        distributed,
        instructors,
        ordered_ab,
        ordered_ba
    };

    let obj_distributed: Expression = vars.distributed.iter().copied().sum();

    let obj_instructors: Expression = vars
        .instructors
        .iter()
        .enumerate()
        .flat_map(|(a, row)| {
            row.iter().enumerate().filter_map(move |(i, v)| {
                v.filter(|_| {
                    fractivity_entries[a]
                        .0
                        .instructor_extension_uuids
                        .contains(&all_instructors[i])
                })
            })
        })
        .sum();

    let obj_visitors: Expression = vars
        .instructors
        .iter()
        .enumerate()
        .flat_map(|(a, row)| {
            row.iter().enumerate().filter_map(move |(i, v)| {
                v.filter(|_| {
                    fractivity_entries[a]
                        .0
                        .visitor_extension_uuids
                        .contains(&all_instructors[i])
                })
            })
        })
        .sum();

    let obj_ordered = vars.ordered_ab.iter().sum::<Expression>() + vars.ordered_ba.iter().sum::<Expression>();


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
        &start_information,
        &pair_information
    )?;

    println!("Found solution");
    let mut result_distributed = Vec::new();
    let mut err_undistributed = Vec::new();

    for a in 0..fractivity_entries.len() {
        let mut room_dist = None;
        for r in 0..all_rooms.len() {
            if vars.rooms[a][r].is_some() && solution.value(vars.rooms[a][r].unwrap()).round() > 0.0 {
                room_dist = Some(all_rooms[r].id);
                break;
            }
        }
        let mut undistributed_instructors = Vec::new();
        for i in 0..all_instructors.len() {
            if fractivity_entries[a]
                .0
                .instructor_extension_uuids
                .contains(&all_instructors[i])
                && solution.value(vars.instructors[a][i].unwrap()).round() == 0.0
            {
                undistributed_instructors.push(all_instructors[i]);
            }
        }
        if room_dist.is_none() || solution.value(vars.distributed[a]).round() == 0.0 {
            err_undistributed.push((
                fractivity_entries[a].0.clone(),
                FractivityDistributionFailReason::RoomUnavailable,
            ));
        } else if !undistributed_instructors.is_empty() {
            err_undistributed.push((
                fractivity_entries[a].0.clone(),
                FractivityDistributionFailReason::InstructorUnavailable(undistributed_instructors),
            ));
        } else {
            result_distributed.push((
                a,
                room_dist.unwrap(),
                solution.value(vars.starts[a]).round() as i32,
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
