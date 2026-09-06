use core::time;
use std::{
    cmp::{max, min},
    collections::HashSet,
};

mod distribution_checks;

use chrono::NaiveDate;
use uuid::Uuid;

use good_lp::{
    constraint, default_solver, variable, variables, Expression, IntoAffineExpression,
    ProblemVariables, Solution, SolverModel, Variable, WithMipGap,
};

use gcd::Gcd;

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

fn occupies(a: &FractivityEntry, s: i32, t: i32) -> bool {
    t >= s - a.preparation_time && t < s + a.duration + a.follow_up_time
}

fn overlap(a1: &FractivityEntry, s1: i32, a2: &FractivityEntry, s2: i32) -> bool {
    occupies(a1, s1, s2 - a2.preparation_time) || occupies(a2, s2, s1 - a1.preparation_time)
}

pub fn distribute(
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
    all_rooms: Vec<FractivityRoom>,
) -> Result<Result<Vec<(FractivityEntry, Uuid, i32)>, Vec<FractivityEntry>>, String> {
    let mut instructors_aggregator = HashSet::new();
    let mut rooms_aggregator = HashSet::new();
    let mut first_start = i32::MAX;
    let mut last_start = i32::MIN;
    let mut last_finish = i32::MIN;
    let mut time_resolution = 30;

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
            return Ok(Err(vec![entry.0.clone()]));
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
        first_start = min(first_start, entry.0.allowed_starts[0]);

        last_start = max(last_start, *entry.0.allowed_starts.last().unwrap());

        last_finish = max(
            last_finish,
            *entry.0.allowed_starts.last().unwrap() + entry.0.duration,
        );

        time_resolution = Gcd::gcd(time_resolution, entry.0.duration as u64);
        if entry.0.preparation_time > 0 {
            time_resolution = Gcd::gcd(time_resolution, entry.0.preparation_time as u64);
        }
        if entry.0.follow_up_time > 0 {
            time_resolution = Gcd::gcd(time_resolution, entry.0.follow_up_time as u64);
        }

        if entry.0.allowed_starts[0] > 0 {
            time_resolution = Gcd::gcd(time_resolution, entry.0.allowed_starts[0] as u64);
        }

        if *entry.0.allowed_starts.last().unwrap() > 0 {
            time_resolution = Gcd::gcd(
                time_resolution,
                *entry.0.allowed_starts.last().unwrap() as u64,
            );
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

    Ok(lp_distribution(
        fractivity_entries,
        &all_rooms,
        &all_instructors,
        first_start,
        last_start,
        last_finish,
        time_resolution as i32,
    )?
    .map(|v| {
        v.iter()
            .map(|(entry_idx, room, s)| (fractivity_entries[*entry_idx].0.clone(), *room, *s))
            .collect()
    }))
}

fn create_fractivity_variables(
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
    all_rooms: &Vec<FractivityRoom>,
    num_instructors: usize,
    all_vars: &mut ProblemVariables,
    objective: &mut Expression,
    max_dist_objective: &mut Expression,
) -> (Vec<Variable>, Vec<Vec<Option<Variable>>>, Vec<Variable>) {
    let mut starts = Vec::new();
    let mut rooms = Vec::new();
    let mut distributed = Vec::new();

    for a in fractivity_entries {
        let v = all_vars.add(variable().integer());
        starts.push(v);
        let d = all_vars.add(variable().binary());
        distributed.push(d);
        if a.0.blocker {
            max_dist_objective.add_mul((4 * num_instructors) as f64, d);
        } else {
            max_dist_objective.add_mul(
                (2 * num_instructors + a.0.instructor_extension_uuids.len()) as f64,
                d,
            );
        }
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
                        objective.add_mul(r.priority as f64, v);
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

fn create_visitor_variables(
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
    all_instructors: &Vec<Uuid>,
    all_vars: &mut ProblemVariables,
    objective: &mut Expression,
) -> Vec<Vec<Option<Variable>>> {
    let mut v = Vec::new();

    for a in 0..fractivity_entries.len() {
        let mut va = Vec::new();
        for i in all_instructors {
            let mut res = None;
            if fractivity_entries[a].0.visitor_extension_uuids.contains(&i) {
                let var = all_vars.add(variable().binary());
                println!(
                    "adding visitor variable for fractivity {:?} visitor {:?} ",
                    fractivity_entries[a].0.topic, i
                );
                objective.add_mul(
                    (fractivity_entries.len() * all_instructors.len()) as f64,
                    var,
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
    all_rooms: &Vec<FractivityRoom>,
    num_time_steps: i32,
    all_vars: &mut ProblemVariables,
    objective: &mut Expression,
) -> (Vec<Vec<Variable>>, Vec<Vec<Variable>>) {
    let mut fractivity_occupies = Vec::new();

    for _ in 0..num_time_steps {
        let mut fractivity_occupies_time = Vec::new();
        for _ in 0..fractivity_entries.len() {
            fractivity_occupies_time.push(all_vars.add(variable().binary()));
        }
        fractivity_occupies.push(fractivity_occupies_time);
    }

    let mut b = Vec::new();
    let max_parallel: usize = fractivity_entries.len().min(all_rooms.len());
    for _ in 0..num_time_steps {
        let mut bt = Vec::new();
        for k in 0..max_parallel {
            let btk = all_vars.add(variable().binary());
            objective.add_mul(-0.5 * (k as f64 + 1.0) / (max_parallel) as f64, btk);
            bt.push(btk);
        }
        b.push(bt);
    }

    (b, fractivity_occupies)
}

fn lp_distribution(
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
    all_rooms: &Vec<FractivityRoom>,
    all_instructors: &Vec<Uuid>,
    first_start: i32,
    last_start: i32,
    last_finish: i32,
    time_resolution: i32,
) -> Result<Result<Vec<(usize, Uuid, i32)>, Vec<FractivityEntry>>, String> {
    let mut all_vars = variables!();
    let mut objective = Expression::default();
    let num_time_steps = (last_finish - first_start) / time_resolution;
    let (starts, rooms, _) = create_fractivity_variables(
        fractivity_entries,
        all_rooms,
        all_instructors.len(),
        &mut all_vars,
        &mut objective,
        &mut Expression::default(),
    );
    let (instructor_overlap, room_overlap, any_overlap, fractivity_order) =
        create_fractivity_pair_variables(fractivity_entries, &mut all_vars);
    let v = create_visitor_variables(
        fractivity_entries,
        all_instructors,
        &mut all_vars,
        &mut objective,
    );

    let (b, fractivity_occupies) = create_parallel_variables(
        fractivity_entries,
        all_rooms,
        num_time_steps,
        &mut all_vars,
        &mut objective,
    );
    let mut problem = all_vars.clone().maximise(objective).using(default_solver);
    //.with_time_limit(1);

    let mut index = 0;
    // Distribute max once and only in legal rooms and starts
    for a in 0..fractivity_entries.len() {
        problem.add_constraint(constraint!(
            starts[a] >= fractivity_entries[a].0.allowed_starts[0]
        ));
        problem.add_constraint(constraint!(
            starts[a] <= *fractivity_entries[a].0.allowed_starts.last().unwrap()
        ));

        let mut room_constr = Expression::default();
        for r in &rooms[a] {
            if r.is_some() {
                room_constr.add_mul(1.0, r.unwrap());
            }
        }
        problem.add_constraint(constraint!(room_constr.clone() == 1.0));

        for b in (a + 1)..fractivity_entries.len() {
            println!(
                "preventing overlap between {:?} and {:?}",
                fractivity_entries[a].0.id, fractivity_entries[b].0.id
            );

            // fractivity-order = 1 => b NACH a

            problem.add_constraint(constraint!(
                starts[a]
                    + fractivity_order[index] * last_finish
                    + (1 - any_overlap[index]) * last_finish
                    >= starts[b]
                        + fractivity_entries[b].0.duration
                        + fractivity_entries[b].0.follow_up_time
                        + fractivity_entries[a].0.preparation_time
            ));
            problem.add_constraint(constraint!(
                starts[b]
                    + (1 - fractivity_order[index]) * last_finish
                    + (1 - any_overlap[index]) * last_finish
                    >= starts[a]
                        + fractivity_entries[a].0.duration
                        + fractivity_entries[a].0.follow_up_time
                        + fractivity_entries[b].0.preparation_time
            ));

            if fractivity_entries[a].0.blocker || fractivity_entries[b].0.blocker {
                problem.add_constraint(constraint!(any_overlap[index] == 1));
                index += 1;
                continue;
            }

            problem.add_constraint(constraint!(
                2 * any_overlap[index] >= room_overlap[index] + instructor_overlap[index]
            ));

            for r in 0..all_rooms.len() {
                if rooms[a][r].is_some() && rooms[b][r].is_some() {
                    problem.add_constraint(constraint!(
                        room_overlap[index] + 1 >= rooms[a][r].unwrap() + rooms[b][r].unwrap()
                    ));
                }
            }
            let mut i_overlap = false;
            for i1 in &fractivity_entries[a].0.instructor_extension_uuids {
                if fractivity_entries[b]
                    .0
                    .instructor_extension_uuids
                    .contains(i1)
                {
                    problem.add_constraint(constraint!(instructor_overlap[index] == 1));
                    i_overlap = true;
                    break;
                }
            }
            if !i_overlap {
                for i in 0..all_instructors.len() {
                    let id = all_instructors[i];
                    let mut rhs = Expression::default();
                    let mut any_visitor = false;
                    if fractivity_entries[a]
                        .0
                        .instructor_extension_uuids
                        .contains(&id)
                    {
                        rhs.add_mul(1.0, 1.0);
                    } else if v[a][i].is_some() {
                        any_visitor = true;
                        rhs.add_mul(1.0, v[a][i].unwrap());
                    }
                    if fractivity_entries[b]
                        .0
                        .instructor_extension_uuids
                        .contains(&id)
                    {
                        rhs.add_mul(1.0, 1.0);
                    } else if v[b][i].is_some() {
                        any_visitor = true;
                        rhs.add_mul(1.0, v[b][i].unwrap());
                    }
                    if any_visitor {
                        problem.add_constraint(constraint!(instructor_overlap[index] + 1 >= rhs));
                    }
                }
            }

            index += 1;
        }
    }

    let max_parallel = fractivity_entries.len().min(all_rooms.len());
    for t in 0..num_time_steps {
        let cur_time = first_start + time_resolution * t;
        let mut constr = Expression::default();
        for a in 0..fractivity_entries.len() {
            problem.add_constraint(constraint!(
                starts[a] <= cur_time + (1 - fractivity_occupies[t as usize][a]) * last_finish
            ));
            problem.add_constraint(constraint!(
                starts[a]
                    + fractivity_entries[a].0.duration
                    + (1 - fractivity_occupies[t as usize][a]) * last_finish
                    >= cur_time
            ));

            constr.add_mul(1.0, fractivity_occupies[t as usize][a]);
        }
        for k in 0..max_parallel {
            problem.add_constraint(constraint!(
                constr.clone() <= b[t as usize][k] * max_parallel as f64 + (k as f64)
            ));
        }
    }

    match problem.solve() {
        Ok(solution) => {
            println!("Found solution");
            let mut result_distributed = Vec::new();
            let mut err_undistributed = Vec::new();

            let mut index = 0;
            for a in 0..fractivity_entries.len() {
                for b in (a + 1)..fractivity_entries.len() {
                    if solution.value(any_overlap[index]) == 0.0 {
                        index += 1;
                        continue;
                    }

                    println!(
                        "Variables for overlap between {:?} and {:?}",
                        fractivity_entries[a].0.id, fractivity_entries[b].0.id
                    );

                    println!(
                        "Room overlap: {:?} instructor overlap: {:?} any overlap: {:?} order: {:?}",
                        solution.value(room_overlap[index]),
                        solution.value(instructor_overlap[index]),
                        solution.value(any_overlap[index]),
                        solution.value(fractivity_order[index])
                    );
                    index += 1;
                }
            }

            for a in 0..fractivity_entries.len() {
                let mut room_dist = None;
                for r in 0..all_rooms.len() {
                    if rooms[a][r].is_some() && solution.value(rooms[a][r].unwrap()) > 0.0 {
                        room_dist = Some(all_rooms[r].id);
                        break;
                    }
                }
                if room_dist.is_none() {
                    err_undistributed.push(fractivity_entries[a].0.clone());
                } else {
                    result_distributed.push((
                        a,
                        room_dist.unwrap(),
                        (solution.value(starts[a]) / time_resolution as f64).round() as i32
                            * time_resolution,
                    ));
                }
            }
            if err_undistributed.is_empty() {
                Ok(Ok(result_distributed))
            } else {
                Ok(Err(err_undistributed))
            }
        }

        Err(_) => {
            println!("Could not find a fast solution");
            let mut all_vars = variables!();
            let mut objective = Expression::default();
            let (starts, rooms, distributed) = create_fractivity_variables(
                fractivity_entries,
                all_rooms,
                all_instructors.len(),
                &mut all_vars,
                &mut Expression::default(),
                &mut objective,
            );
            let (instructor_overlap, room_overlap, any_overlap, fractivity_order) =
                create_fractivity_pair_variables(fractivity_entries, &mut all_vars);
            let mut problem = all_vars.clone().maximise(objective).using(default_solver);
            //.with_time_limit(1);

            let mut index = 0;
            // Distribute max once and only in legal rooms and starts
            for a in 0..fractivity_entries.len() {
                problem.add_constraint(constraint!(
                    starts[a] >= fractivity_entries[a].0.allowed_starts[0]
                ));
                problem.add_constraint(constraint!(
                    starts[a] <= *fractivity_entries[a].0.allowed_starts.last().unwrap()
                ));

                let mut room_constr = Expression::default();
                for r in &rooms[a] {
                    if r.is_some() {
                        room_constr.add_mul(1.0, r.unwrap());
                    }
                }
                problem.add_constraint(constraint!(room_constr.clone() == 1.0));

                for b in (a + 1)..fractivity_entries.len() {
                    // fractivity-order = 1 => b NACH a

                    problem.add_constraint(constraint!(
                        starts[a]
                            + fractivity_order[index] * last_finish
                            + (1 - any_overlap[index]) * last_finish
                            >= starts[b]
                                + fractivity_entries[b].0.duration
                                + fractivity_entries[b].0.follow_up_time
                                + fractivity_entries[a].0.preparation_time
                    ));
                    problem.add_constraint(constraint!(
                        starts[b]
                            + (1 - fractivity_order[index]) * last_finish
                            + (1 - any_overlap[index]) * last_finish
                            >= starts[a]
                                + fractivity_entries[a].0.duration
                                + fractivity_entries[a].0.follow_up_time
                                + fractivity_entries[b].0.preparation_time
                    ));

                    if fractivity_entries[a].0.blocker || fractivity_entries[b].0.blocker {
                        problem.add_constraint(constraint!(any_overlap[index] + (1 - distributed[a]) + (1 - distributed[b]) >= 1));
                        index += 1;
                        continue;
                    }

                    problem.add_constraint(constraint!(
                        2 * (any_overlap[index] + (1 - distributed[a]) + (1 - distributed[b]))
                            >= room_overlap[index] + instructor_overlap[index]
                    ));

                    for r in 0..all_rooms.len() {
                        if rooms[a][r].is_some() && rooms[b][r].is_some() {
                            problem.add_constraint(constraint!(
                                room_overlap[index] + 1
                                    >= rooms[a][r].unwrap() + rooms[b][r].unwrap()
                            ));
                        }
                    }
                    for i1 in &fractivity_entries[a].0.instructor_extension_uuids {
                        if fractivity_entries[b]
                            .0
                            .instructor_extension_uuids
                            .contains(i1)
                        {
                            problem.add_constraint(constraint!(instructor_overlap[index] == 1));
                            break;
                        }
                    }

                    index += 1;
                }
            }

            match problem.solve() {
                Ok(solution) => {
                    let mut err_undistributed = Vec::new();

                    for a in 0..fractivity_entries.len() {
                        if solution.value(distributed[a]).round() == 0.0 {
                            err_undistributed.push(fractivity_entries[a].0.clone())
                        }
                    }

                    if !err_undistributed.is_empty() {
                        Ok(Err(err_undistributed))
                    } else {
                        Err("Suddenly distributable".to_string())
                    }
                }
                Err(e) => Err(e.to_string()),
            }
        }
    }
}
