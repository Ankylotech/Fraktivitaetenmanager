use std::{
    cmp::{max, min},
    collections::HashSet,
};

use chrono::NaiveDate;
use uuid::Uuid;

use good_lp::{
    constraint, default_solver, solvers::coin_cbc::CoinCbcProblem, variable, variables, Expression,
    IntoAffineExpression, ProblemVariables, Solution, SolverModel, Variable, WithMipGap,
    WithTimeLimit,
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
}

const TIME_RESOLUTION: i32 = 15;

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

        last_start = max(
            last_start,
            entry.0.allowed_starts[entry.0.allowed_starts.len() - 1],
        );
    }

    let num_starts = ((last_start - first_start) / TIME_RESOLUTION) + 1;

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
        num_starts,
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
    all_instructors: &Vec<Uuid>,
    first_start: i32,
    num_starts: i32,
    all_vars: &mut ProblemVariables,
    objective: &mut Expression,
) -> (Vec<Vec<Option<Variable>>>, Vec<Vec<Option<Variable>>>) {
    let mut starts = Vec::new();
    let mut rooms = Vec::new();

    for a in fractivity_entries {
        let mut st = Vec::new();
        for s in 0..num_starts {
            let cur_start = first_start + TIME_RESOLUTION * s;
            let mut res = None;
            match a.1 {
                Some((_, start)) => {
                    if cur_start == start {
                        let v = all_vars.add(variable().binary());
                        objective.add_mul(
                            (fractivity_entries.len()
                                * fractivity_entries.len()
                                * all_instructors.len()) as f64,
                            v,
                        );
                        res = Some(v);
                    }
                }
                None => {
                    if a.0.allowed_starts.contains(&cur_start) {
                        let v = all_vars.add(variable().binary());
                        objective
                            .add_mul((fractivity_entries.len() * all_instructors.len()) as f64, v);
                        res = Some(v);
                    }
                }
            }

            st.push(res);
        }
        starts.push(st);
        let mut room = Vec::new();
        for r in all_rooms {
            let mut res = None;
            match a.1 {
                Some((id, _)) => {
                    if id == r.id {
                        let v = all_vars.add(variable().binary());
                        objective.add_mul(
                            (fractivity_entries.len()
                                * fractivity_entries.len()
                                * all_instructors.len()) as f64,
                            v,
                        );
                        res = Some(v);
                    }
                }
                None => {
                    if a.0.allowed_rooms.contains(&r.id) {
                        let v = all_vars.add(variable().binary());
                        objective
                            .add_mul((fractivity_entries.len() * all_instructors.len()) as f64, v);
                        res = Some(v);
                    }
                }
            }

            room.push(res);
        }
        rooms.push(room)
    }

    (starts, rooms)
}

fn create_fractivity_pair_variables(
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
    all_vars: &mut ProblemVariables,
) -> (Vec<Variable>, Vec<Variable>, Vec<Variable>) {
    let mut instructor_overlap = Vec::new();
    let mut room_overlap = Vec::new();
    let mut any_overlap = Vec::new();
    for a in 0..fractivity_entries.len() {
        for _ in (a + 1)..fractivity_entries.len() {
            let i = all_vars.add(variable().binary());
            instructor_overlap.push(i);
            let r = all_vars.add(variable().binary());
            room_overlap.push(r);
            let o = all_vars.add(variable().binary());
            any_overlap.push(o);
        }
    }
    (instructor_overlap, room_overlap, any_overlap)
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
                objective.add_mul(1.0, var);
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
    num_starts: i32,
    all_vars: &mut ProblemVariables,
    objective: &mut Expression,
) -> Vec<Vec<Variable>> {
    let mut b = Vec::new();
    let max_parallel = fractivity_entries.len().min(all_rooms.len());
    for _ in 0..num_starts {
        let mut bt = Vec::new();
        for k in 0..max_parallel {
            let btk = all_vars.add(variable().binary());
            objective.add_mul(-0.5 * (k as f64 + 1.0) / (max_parallel) as f64, btk);
            bt.push(btk);
        }
        b.push(bt);
    }

    b
}

pub fn add_constraints(
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
    all_rooms: &Vec<FractivityRoom>,
    all_instructors: &Vec<Uuid>,
    first_start: i32,
    num_starts: i32,
    problem: &mut CoinCbcProblem,
    starts: &Vec<Vec<Option<Variable>>>,
    rooms: &Vec<Vec<Option<Variable>>>,
    instructor_overlap: &Vec<Variable>,
    room_overlap: &Vec<Variable>,
    any_overlap: &Vec<Variable>,
    b: &Vec<Vec<Variable>>,
    v: &Vec<Vec<Option<Variable>>>,
    allow_undistributed: bool,
) {
    let mut index = 0;
    // Distribute max once and only in legal rooms and starts
    for a in 0..fractivity_entries.len() {
        let mut start_constr = Expression::default();
        for s in &starts[a] {
            if s.is_some() {
                start_constr.add_mul(1.0, s.unwrap());
            }
        }
        let mut room_constr = Expression::default();
        for r in &rooms[a] {
            if r.is_some() {
                room_constr.add_mul(1.0, r.unwrap());
            }
        }
        if allow_undistributed {
            problem.add_constraint(constraint!(start_constr.clone() <= 1.0));
            problem.add_constraint(constraint!(room_constr.clone() <= 1.0));
        } else {
            problem.add_constraint(constraint!(start_constr.clone() == 1.0));
            problem.add_constraint(constraint!(room_constr.clone() == 1.0));
        }

        for b in (a + 1)..fractivity_entries.len() {
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
            for i in 0..all_instructors.len() {
                let id = all_instructors[i];
                let mut rhs = Expression::default();
                if fractivity_entries[a]
                    .0
                    .instructor_extension_uuids
                    .contains(&id)
                {
                    rhs.add_mul(1.0, 1.0);
                } else if !allow_undistributed && v[a][i].is_some() {
                    rhs.add_mul(1.0, v[a][i].unwrap());
                }
                if fractivity_entries[b]
                    .0
                    .instructor_extension_uuids
                    .contains(&id)
                {
                    rhs.add_mul(1.0, 1.0);
                } else if !allow_undistributed && v[b][i].is_some() {
                    rhs.add_mul(1.0, v[b][i].unwrap());
                }
                if rhs.clone().linear_coefficients().len() > 1 {
                    problem.add_constraint(constraint!(instructor_overlap[index] + 1 >= rhs));
                }
            }

            for s1 in 0..num_starts {
                let cur_start1 = first_start + TIME_RESOLUTION * s1;
                if starts[a][s1 as usize].is_none() {
                    continue;
                }
                for s2 in 0..num_starts {
                    let cur_start2 = first_start + TIME_RESOLUTION * s2;
                    if starts[b][s2 as usize].is_none()
                        || !overlap(
                            &fractivity_entries[a].0,
                            cur_start1,
                            &fractivity_entries[b].0,
                            cur_start2,
                        )
                    {
                        continue;
                    }
                    problem.add_constraint(constraint!(
                        starts[a][s1 as usize].unwrap() + starts[b][s2 as usize].unwrap()
                            <= 2 - any_overlap[index]
                    ));
                }
            }

            index += 1;
        }
    }

    let max_parallel = fractivity_entries.len().min(all_rooms.len());
    if !allow_undistributed {
        for t in 0..num_starts {
            let cur_time = first_start + TIME_RESOLUTION * t;
            let mut constr = Expression::default();
            for a in 0..fractivity_entries.len() {
                for s in 0..num_starts {
                    let cur_start = first_start + TIME_RESOLUTION * s;
                    if !fractivity_entries[a].0.allowed_starts.contains(&cur_start) {
                        continue;
                    }
                    if occupies(&fractivity_entries[a].0, cur_start, cur_time)
                        && starts[a][s as usize].is_some()
                    {
                        constr.add_mul(1.0, starts[a][s as usize].unwrap());
                    }
                }
            }
            for k in 0..max_parallel {
                problem.add_constraint(constraint!(
                    constr.clone() <= b[t as usize][k] * max_parallel as f64 + (k as f64)
                ));
            }
        }
    }
}

pub fn lp_distribution(
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
    all_rooms: &Vec<FractivityRoom>,
    all_instructors: &Vec<Uuid>,
    first_start: i32,
    num_starts: i32,
) -> Result<Result<Vec<(usize, Uuid, i32)>, Vec<FractivityEntry>>, String> {
    let mut all_vars = variables!();
    let mut max_distribution_obj = Expression::default();
    let (starts, rooms) = create_fractivity_variables(
        fractivity_entries,
        all_rooms,
        all_instructors,
        first_start,
        num_starts,
        &mut all_vars,
        &mut max_distribution_obj,
    );
    let (instructor_overlap, room_overlap, any_overlap) =
        create_fractivity_pair_variables(fractivity_entries, &mut all_vars);
    let mut parallel_obj = Expression::default();
    let mut visitor_obj = Expression::default();
    let b = create_parallel_variables(
        fractivity_entries,
        all_rooms,
        num_starts,
        &mut all_vars,
        &mut parallel_obj,
    );
    let v = create_visitor_variables(
        fractivity_entries,
        all_instructors,
        &mut all_vars,
        &mut visitor_obj,
    );

    let mut problem = all_vars
        .clone()
        .maximise(max_distribution_obj)
        .using(default_solver);
    //.with_time_limit(1);

    add_constraints(
        fractivity_entries,
        all_rooms,
        all_instructors,
        first_start,
        num_starts,
        &mut problem,
        &starts,
        &rooms,
        &instructor_overlap,
        &room_overlap,
        &any_overlap,
        &b,
        &v,
        true,
    );

    match problem.solve() {
        Ok(solution) => {
            println!("Found solution to initial LP");
            let mut err_undistributed = Vec::new();

            for a in 0..fractivity_entries.len() {
                let mut dist = (false, false);
                for s in 0..num_starts {
                    if starts[a][s as usize].is_some()
                        && solution.value(starts[a][s as usize].unwrap()) > 0.0
                    {
                        dist.0 = true;
                        break;
                    }
                }
                for r in 0..all_rooms.len() {
                    if rooms[a][r].is_some() && solution.value(rooms[a][r].unwrap()) > 0.0 {
                        dist.1 = true;
                        break;
                    }
                }
                if !dist.0 || !dist.1 {
                    err_undistributed.push(fractivity_entries[a].0.clone());
                }
            }

            if !err_undistributed.is_empty() {
                Ok(Err(err_undistributed))
            } else {
                problem = match all_vars
                    .clone()
                    .maximise(visitor_obj.clone())
                    .using(default_solver)
                    .with_mip_gap(0.5)
                {
                    Ok(o) => o,
                    Err(e) => return Err(e.to_string()),
                };
                add_constraints(
                    fractivity_entries,
                    all_rooms,
                    all_instructors,
                    first_start,
                    num_starts,
                    &mut problem,
                    &starts,
                    &rooms,
                    &instructor_overlap,
                    &room_overlap,
                    &any_overlap,
                    &b,
                    &v,
                    false,
                );

                match problem.solve() {
                    Ok(solution1) => {
                        problem = all_vars
                            .maximise(parallel_obj)
                            .using(default_solver)
                            .with_time_limit(1)
                            .with_mip_gap(0.5)
                            .unwrap();
                        add_constraints(
                            fractivity_entries,
                            all_rooms,
                            all_instructors,
                            first_start,
                            num_starts,
                            &mut problem,
                            &starts,
                            &rooms,
                            &instructor_overlap,
                            &room_overlap,
                            &any_overlap,
                            &b,
                            &v,
                            false,
                        );
                        let mut visitor_sum = Expression::default();
                        for v1 in v {
                            for v2 in v1 {
                                if v2.is_some() {
                                    visitor_sum.add_mul(1.0, v2.unwrap());
                                }
                            }
                        }
                        problem.add_constraint(constraint!(
                            visitor_sum >= solution1.eval(visitor_obj)
                        ));

                        match problem.solve() {
                            Ok(solution2) => {
                                let mut result_distributed = Vec::new();
                                let mut err_undistributed = Vec::new();

                                for a in 0..fractivity_entries.len() {
                                    let mut dist = (None, None);
                                    for s in 0..num_starts {
                                        let cur_start = first_start + TIME_RESOLUTION * s;
                                        if starts[a][s as usize].is_some()
                                            && solution2.value(starts[a][s as usize].unwrap()) > 0.0
                                        {
                                            dist.0 = Some(cur_start);
                                            break;
                                        }
                                    }
                                    for r in 0..all_rooms.len() {
                                        if rooms[a][r].is_some()
                                            && solution2.value(rooms[a][r].unwrap()) > 0.0
                                        {
                                            dist.1 = Some(all_rooms[r].id);
                                            break;
                                        }
                                    }
                                    if dist.0.is_none() || dist.1.is_none() {
                                        err_undistributed.push(fractivity_entries[a].0.clone());
                                    } else {
                                        result_distributed.push((
                                            a,
                                            dist.1.unwrap(),
                                            dist.0.unwrap(),
                                        ));
                                    }
                                }
                                if err_undistributed.is_empty() {
                                    Ok(Ok(result_distributed))
                                } else {
                                    Ok(Err(err_undistributed))
                                }
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    Err(e) => Err(e.to_string()),
                }
            }
        }
        Err(e) => Err(e.to_string()),
    }
}
