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
    room_overlap: Vec<Variable>,
    instructor_overlap: Vec<Variable>,
    any_overlap: Vec<Variable>,
    fractivity_order: Vec<Variable>,
    fractivity_occupies: Vec<Vec<Variable>>, // [t][a]
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
) -> (Vec<Variable>, Vec<Vec<Option<Variable>>>, Vec<Variable>) {
    let mut starts = Vec::new();
    let mut rooms = Vec::new();
    let mut distributed = Vec::new();

    for a in fractivity_entries {
        let v = all_vars.add(variable().integer());
        starts.push(v);
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
        rooms.push(room)
    }

    (starts, rooms, distributed)
}

fn create_fractivity_pair_variables(
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
    all_vars: &mut ProblemVariables,
) -> (Vec<Variable>, Vec<Variable>, Vec<Variable>, Vec<Variable>) {
    let mut instructor_overlap = Vec::new();
    let mut room_overlap = Vec::new();
    let mut any_overlap = Vec::new();
    let mut fractivity_order = Vec::new();
    for a in 0..fractivity_entries.len() {
        for _ in (a + 1)..fractivity_entries.len() {
            let i = all_vars.add(variable().binary());
            instructor_overlap.push(i);
            let r = all_vars.add(variable().binary());
            room_overlap.push(r);
            let o = all_vars.add(variable().binary());
            any_overlap.push(o);
            let order = all_vars.add(variable().binary());
            fractivity_order.push(order);
        }
    }
    (
        instructor_overlap,
        room_overlap,
        any_overlap,
        fractivity_order,
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
                println!(
                    "adding instructor variable for fractivity {:?} visitor {:?} ",
                    fractivity_entries[a].0.topic, i
                );
                res = Some(var);
            } else if fractivity_entries[a].0.visitor_extension_uuids.contains(&i) {
                let var = all_vars.add(variable().binary());
                println!(
                    "adding visitor variable for fractivity {:?} visitor {:?} ",
                    fractivity_entries[a].0.topic, i
                );
                res = Some(var);
            }

            va.push(res);
        }
        v.push(va);
    }
    v
}

fn create_parallel_variables(
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
    all_vars: &mut ProblemVariables,
) -> Vec<Vec<Variable>> {
    let mut fractivity_occupies = Vec::new();

    for _ in 0..fractivity_entries.len() {
        let mut fractivity_occupies_time = Vec::new();
        for _ in 0..fractivity_entries.len() {
            let v = all_vars.add(variable().binary());
            fractivity_occupies_time.push(v);
        }
        fractivity_occupies.push(fractivity_occupies_time);
    }
    fractivity_occupies
}

fn add_constraints(
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
    all_rooms: &Vec<FractivityRoom>,
    all_instructors: &Vec<Uuid>,
    problem: &mut HighsProblem,
    vars: &ModelVars,
) {
    let mut index = 0;
    // Distribute max once and only in legal rooms and starts
    for a in 0..fractivity_entries.len() {
        problem.add_constraint(constraint!(
            vars.starts[a] >= fractivity_entries[a].0.allowed_starts[0]
        ));
        problem.add_constraint(constraint!(
            vars.starts[a] <= *fractivity_entries[a].0.allowed_starts.last().unwrap()
        ));

        let mut room_constr = Expression::default();
        for r in &vars.rooms[a] {
            if r.is_some() {
                room_constr.add_mul(1.0, r.unwrap());
            }
        }
        problem.add_constraint(constraint!(room_constr.clone() == vars.distributed[a]));

        for b in (a + 1)..fractivity_entries.len() {
            // fractivity-order = 1 => b NACH a

            problem.add_constraint(constraint!(
                vars.starts[a]
                    + vars.fractivity_order[index]
                        * (fractivity_entries[b].0.allowed_starts.last().unwrap()
                            + fractivity_entries[b].0.duration
                            + fractivity_entries[b].0.follow_up_time
                            + fractivity_entries[a].0.preparation_time)
                    + (1 - vars.any_overlap[index])
                        * (fractivity_entries[b].0.allowed_starts.last().unwrap()
                            + fractivity_entries[b].0.duration
                            + fractivity_entries[b].0.follow_up_time
                            + fractivity_entries[a].0.preparation_time)
                    >= vars.starts[b]
                        + fractivity_entries[b].0.duration
                        + fractivity_entries[b].0.follow_up_time
                        + fractivity_entries[a].0.preparation_time
            ));
            problem.add_constraint(constraint!(
                vars.starts[b]
                    + (1 - vars.fractivity_order[index])
                        * (fractivity_entries[a].0.allowed_starts.last().unwrap()
                            + fractivity_entries[a].0.duration
                            + fractivity_entries[a].0.follow_up_time
                            + fractivity_entries[b].0.preparation_time)
                    + (1 - vars.any_overlap[index])
                        * (fractivity_entries[a].0.allowed_starts.last().unwrap()
                            + fractivity_entries[a].0.duration
                            + fractivity_entries[a].0.follow_up_time
                            + fractivity_entries[b].0.preparation_time)
                    >= vars.starts[a]
                        + fractivity_entries[a].0.duration
                        + fractivity_entries[a].0.follow_up_time
                        + fractivity_entries[b].0.preparation_time
            ));

            if fractivity_entries[a].0.blocker || fractivity_entries[b].0.blocker {
                problem.add_constraint(constraint!(
                    vars.any_overlap[index] + (1 - vars.distributed[a]) + (1 - vars.distributed[b])
                        >= 1
                ));
                index += 1;
                continue;
            }

            problem.add_constraint(constraint!(
                vars.any_overlap[index] + (1 - vars.distributed[a]) + (1 - vars.distributed[b])
                    >= vars.room_overlap[index]
            ));
            problem.add_constraint(constraint!(
                vars.any_overlap[index] + (1 - vars.distributed[a]) + (1 - vars.distributed[b])
                    >= vars.instructor_overlap[index]
            ));

            for r in 0..all_rooms.len() {
                if vars.rooms[a][r].is_some() && vars.rooms[b][r].is_some() {
                    problem.add_constraint(constraint!(
                        vars.room_overlap[index] + 1
                            >= vars.rooms[a][r].unwrap() + vars.rooms[b][r].unwrap()
                    ));
                }
            }

            for i in 0..all_instructors.len() {
                let mut rhs = Expression::default();
                let mut any_visitor = false;
                if vars.instructors[a][i].is_some() {
                    any_visitor = true;
                    rhs.add_mul(1.0, vars.instructors[a][i].unwrap());
                }
                if vars.instructors[b][i].is_some() {
                    any_visitor = true;
                    rhs.add_mul(1.0, vars.instructors[b][i].unwrap());
                }
                if any_visitor {
                    problem.add_constraint(constraint!(vars.instructor_overlap[index] + 1 >= rhs));
                }
            }

            index += 1;
        }
    }

    for t in 0..fractivity_entries.len() {
        for a in 0..fractivity_entries.len() {
            problem.add_constraint(constraint!(
                vars.starts[a]
                    <= vars.starts[t]
                        + (vars.fractivity_occupies[t as usize][a])
                            * *fractivity_entries[a].0.allowed_starts.last().unwrap()
            ));
            problem.add_constraint(constraint!(
                vars.starts[a]
                    + fractivity_entries[a].0.duration
                    + (vars.fractivity_occupies[t as usize][a])
                        * *(fractivity_entries[t].0.allowed_starts.last().unwrap())
                    >= vars.starts[t]
            ));
        }
    }
}

fn solve_stage(
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
    all_rooms: &Vec<FractivityRoom>,
    all_instructors: &Vec<Uuid>,
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
        .set_mip_abs_gap(parallel_total * 0.5)
        .map_err(|e| e.to_string())?;

    problem.set_verbose(true);

    add_constraints(
        fractivity_entries,
        all_rooms,
        all_instructors,
        &mut problem,
        vars,
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
    let fractivity_occupies = create_parallel_variables(fractivity_entries, &mut all_vars);
    let instructors =
        create_instructor_variables(fractivity_entries, all_instructors, &mut all_vars);
    let (starts, rooms, distributed) =
        create_fractivity_variables(fractivity_entries, all_rooms, &mut all_vars);
    let (instructor_overlap, room_overlap, any_overlap, fractivity_order) =
        create_fractivity_pair_variables(fractivity_entries, &mut all_vars);

    let vars = ModelVars {
        starts,
        rooms,
        distributed,
        instructors,
        room_overlap,
        instructor_overlap,
        any_overlap,
        fractivity_order,
        fractivity_occupies,
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

    let obj_parallel: Expression = vars
        .fractivity_occupies
        .iter()
        .map(|f| f.iter().copied().sum::<Expression>())
        .sum();

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
    total_objective *= obj_parallel
        .clone()
        .linear_coefficients()
        .map(|(_, p)| p)
        .sum::<f64>()
        + 1.0;
    total_objective -= obj_parallel.clone();
    let parallel_total = obj_parallel
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
        all_rooms,
        all_instructors,
        &all_vars,
        &vars,
        total_objective,
        ObjectiveDirection::Maximisation,
        parallel_total as f32,
    )?;

    println!("Found solution");
    let mut result_distributed = Vec::new();
    let mut err_undistributed = Vec::new();

    let mut index = 0;
    for a in 0..fractivity_entries.len() {
        println!(
            "Fractivity {:?} distributed: {:?}",
            fractivity_entries[a].0.id,
            solution.value(vars.distributed[a])
        );
        for b in (a + 1)..fractivity_entries.len() {
            if solution.value(vars.any_overlap[index]) == 0.0 {
                index += 1;
                continue;
            }

            println!(
                "Variables for overlap between {:?} and {:?}",
                fractivity_entries[a].0.id, fractivity_entries[b].0.id
            );

            println!(
                "Room overlap: {:?} instructor overlap: {:?} any overlap: {:?} order: {:?}",
                solution.value(vars.room_overlap[index]),
                solution.value(vars.instructor_overlap[index]),
                solution.value(vars.any_overlap[index]),
                solution.value(vars.fractivity_order[index])
            );
            index += 1;
        }
    }

    for a in 0..fractivity_entries.len() {
        let mut room_dist = None;
        for r in 0..all_rooms.len() {
            if vars.rooms[a][r].is_some() && solution.value(vars.rooms[a][r].unwrap()) > 0.0 {
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
