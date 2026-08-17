use crate::models::FractivityEntry;
use crate::models::FractivityRoom;
use good_lp::{
    constraint, default_solver, variable, variables, Expression, ObjectiveDirection, Solution,
    SolverModel, WithMipGap, 
};

use std::collections::HashMap;
use std::{cmp::max, cmp::min, collections::HashSet};
use uuid::Uuid;

const TIME_RESOLUTION: i32 = 15;

fn occupies(a: &FractivityEntry, s: i32, t: i32) -> bool {
    return t >= s - a.preparation_time && t < s + a.duration + a.follow_up_time;
}


pub fn distribute(
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
) -> Result<Result<Vec<(FractivityEntry, Uuid, i32)>, Vec<FractivityEntry>>, String> {
    let mut instructors_aggregator = HashSet::new();
    let mut rooms_aggregator = HashSet::new();
    let mut first_start = i32::MAX;
    let mut last_start = i32::MIN;

    for entry in fractivity_entries {
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

    // TODO technically we only need from one event here. Makes this more efficient
    let room_meta: HashMap<Uuid, FractivityRoom> = match FractivityRoom::find_all() {
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
        .collect::<Result<Vec<FractivityRoom>,String>>()?;

    Ok(lp_distribution(
            fractivity_entries,
            &all_rooms,
            &all_instructors,
            first_start,
            num_starts,
    )?.map(|v| v
        .iter()
        .map(|(entry_idx, room, s)| (fractivity_entries[*entry_idx].0.clone(), *room, *s))
        .collect()))
}
pub fn lp_distribution(
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
    all_rooms: &Vec<FractivityRoom>,
    all_instructors: &Vec<Uuid>,
    first_start: i32,
    num_starts: i32,
) -> Result<Result<Vec<(usize, Uuid, i32)>, Vec<FractivityEntry>>, String> {
    let mut all_vars = variables!();
    let mut obj = Expression::default();
    let max_parallel = fractivity_entries.len().min(all_rooms.len());

    let mut x = Vec::new();
    for a in 0..fractivity_entries.len() {
        let mut xa = Vec::new();
        for s in 0..num_starts {
            let cur_start = first_start + TIME_RESOLUTION * s;
            let mut xs = Vec::new();
            for r in all_rooms {
                let mut res = None;
                match fractivity_entries[a].1 {
                    Some((id, start)) => {
                        if r.id == id && cur_start == start {
                            let v = all_vars.add(variable().integer().min(1).max(1));
                            res = Some(v);
                        }
                    }
                    None => {
                        if fractivity_entries[a].0.allowed_rooms.contains(&r.id)
                            && fractivity_entries[a].0.allowed_starts.contains(&cur_start)
                        {
                            let v = all_vars.add(variable().binary().initial(0));
                            obj.add_mul(
                                r.priority as f64
                                    / (fractivity_entries.len()
                                        * all_instructors.len()
                                        * max_parallel
                                        * max_parallel)
                                        as f64,
                                v,
                            );
                            res = Some(v);
                        }
                    }
                }
                xs.push(res);
            }
            xa.push(xs);
        }

        x.push(xa);
    }
    let mut b = Vec::new();
    let mut v = Vec::new();
    for a in 0..fractivity_entries.len() {
        let mut va = Vec::new();
        for i in all_instructors {
            let mut vv = Vec::new();
            for s in 0..num_starts {
                let cur_start = first_start + TIME_RESOLUTION * s;
                let mut res = None;
                if fractivity_entries[a].0.visitor_extension_uuids.contains(&i)
                    && fractivity_entries[a].0.allowed_starts.contains(&cur_start)
                {
                    let var = all_vars.add(variable().binary().initial(0));
                    obj.add_mul(1.0, var);
                    res = Some(var);
                }

                vv.push(res);
            }
            va.push(vv);
        }
        v.push(va);
    }
    for _ in 0..num_starts {
        let mut bt = Vec::new();
        for k in 0..max_parallel {
            let btk = all_vars.add(variable().binary().initial(1));
            obj.add_mul(
                -0.5 * (k as f64)
                    / (fractivity_entries.len() * all_instructors.len() * max_parallel) as f64,
                btk,
            );
            bt.push(btk);
        }
        b.push(bt);
    }

    let mut problem = all_vars
        .optimise(ObjectiveDirection::Maximisation, obj)
        .using(default_solver)
        .with_mip_gap(0.5)
        .unwrap();
        //.with_time_limit(1);

    //problem.set_verbose(true);

    // Distribute max once and only in legal rooms and starts
    for a in 0..fractivity_entries.len() {
        let mut constr = Expression::default();
        for s in 0..num_starts {
            for r in 0..all_rooms.len() {
                if x[a][s as usize][r].is_none() {
                    continue;
                }
                constr.add_mul(1.0, x[a][s as usize][r].unwrap());
            }
        }
        problem.add_constraint(constraint!(constr == 1));
        for i in 0..all_instructors.len() {
            for s in 0..num_starts {
                if v[a][i][s as usize].is_none() {
                    continue;
                }
                constr = Expression::default();
                for r in 0..all_rooms.len() {
                    if x[a][s as usize][r].is_some() {
                        constr.add_mul(1.0, x[a][s as usize][r].unwrap());
                    }
                }
                problem.add_constraint(constraint!(v[a][i][s as usize].unwrap() <= constr));
            }
        }
    }

    for t in 0..num_starts {
        let cur_time = first_start + TIME_RESOLUTION * t;
        // Only one fractivity per room and time t
        for r in 0..all_rooms.len() {
            let mut constr = Expression::default();
            let mut add = 0;
            for a in 0..fractivity_entries.len() {
                for s in 0..num_starts {
                    let cur_start = first_start + TIME_RESOLUTION * s;
                    if occupies(&fractivity_entries[a].0, cur_start, cur_time)
                        && x[a][s as usize][r].is_some()
                    {
                        constr.add_mul(1.0, x[a][s as usize][r].unwrap());
                        add += 1;
                    }
                }
            }
            if add <= 1 {
                continue;
            }
            problem.add_constraint(constraint!(constr <= 1));
        }
        // Only one fractivity per instructor and time t
        for i in 0..all_instructors.len() {
            let mut constr = Expression::default();
            let mut add = 0;
            for a in 0..fractivity_entries.len() {
                if fractivity_entries[a]
                    .0
                    .instructor_extension_uuids
                    .contains(&all_instructors[i])
                {
                    for r in 0..all_rooms.len() {
                        if !fractivity_entries[a]
                            .0
                            .allowed_rooms
                            .contains(&all_rooms[r].id)
                        {
                            continue;
                        }
                        for s in 0..num_starts {
                            let cur_start = first_start + TIME_RESOLUTION * s;
                            if !fractivity_entries[a].0.allowed_starts.contains(&cur_start) {
                                continue;
                            }
                            if occupies(&fractivity_entries[a].0, cur_start, cur_time)
                                && x[a][s as usize][r].is_some()
                            {
                                constr.add_mul(1.0, x[a][s as usize][r].unwrap());
                                add += 1;
                            }
                        }
                    }
                }
                if fractivity_entries[a]
                    .0
                    .visitor_extension_uuids
                    .contains(&all_instructors[i])
                {
                    for s in 0..num_starts {
                        let cur_start = first_start + TIME_RESOLUTION * s;
                        if !fractivity_entries[a].0.allowed_starts.contains(&cur_start) {
                            continue;
                        }
                        if occupies(&fractivity_entries[a].0, cur_start, cur_time)
                            && v[a][i][s as usize].is_some()
                        {
                            constr.add_mul(1.0, v[a][i][s as usize].unwrap());
                            add += 1;
                        }
                    }
                }
            }
            if add <= 1 {
                continue;
            }
            problem.add_constraint(constraint!(constr <= 1));
        }
        let mut constr = Expression::default();
        for a in 0..fractivity_entries.len() {
            for r in 0..all_rooms.len() {
                if !fractivity_entries[a]
                    .0
                    .allowed_rooms
                    .contains(&all_rooms[r].id)
                {
                    continue;
                }
                for s in 0..num_starts {
                    let cur_start = first_start + TIME_RESOLUTION * s;
                    if !fractivity_entries[a].0.allowed_starts.contains(&cur_start) {
                        continue;
                    }
                    if occupies(&fractivity_entries[a].0, cur_start, cur_time)
                        && x[a][s as usize][r].is_some()
                    {
                        constr.add_mul(1.0, x[a][s as usize][r].unwrap());
                    }
                }
            }
        }
        for k in 0..max_parallel {
            problem.add_constraint(constraint!(
                constr.clone() <= b[t as usize][k] * max_parallel as f64 + (k as f64 - 1.0)
            ));
        }
    }

    match problem.solve() {
        Ok(solution) => {
            debug!("Found solution to initial LP");
            let mut result_distributed = Vec::new();
            let mut undistributed_topics = String::new();

            for a in 0..fractivity_entries.len() {
                let mut fract_distributed = false;
                for s in 0..num_starts {
                    let cur_start = first_start + TIME_RESOLUTION * s;
                    for r in 0..all_rooms.len() {
                        if x[a][s as usize][r].is_some()
                            && solution.value(x[a][s as usize][r].unwrap()) > 0.0
                        {
                            fract_distributed = true;
                            result_distributed.push((
                                a,
                                all_rooms[r].id,
                                cur_start,
                            ));
                        }
                    }
                }
                if !fract_distributed {
                    undistributed_topics.push_str(&fractivity_entries[a].0.topic);
                }
            }

            if !undistributed_topics.is_empty() {
                
                Err(undistributed_topics)
            } else {
                Ok(Ok(result_distributed))
            }
        }
        Err(_) => Ok(Err( find_undistributable(
            fractivity_entries,
            all_rooms,
            all_instructors,
            first_start,
            num_starts,
        )?)),
    }
}

fn find_undistributable(
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
    all_rooms: &Vec<FractivityRoom>,
    all_instructors: &Vec<Uuid>,
    first_start: i32,
    num_starts: i32,
) -> Result<Vec<FractivityEntry>, String> {
    debug!("Searching for Entries that cannot be distributed");
    let mut all_vars = variables!();
    let mut obj = Expression::default();
    let mut x = Vec::new();
    for a in 0..fractivity_entries.len() {
        let mut xa = Vec::new();
        for s in 0..num_starts {
            let cur_start = first_start + TIME_RESOLUTION * s;
            let mut xs = Vec::new();
            for r in all_rooms {
                let mut res = None;
                match fractivity_entries[a].1 {
                    Some((id, start)) => {
                        if r.id == id && cur_start == start {
                            let v = all_vars.add(variable().binary().initial(1));
                            obj.add_mul(fractivity_entries.len() as f64, v);
                            res = Some(v);
                        }
                    }
                    None => {
                        if fractivity_entries[a].0.allowed_rooms.contains(&r.id)
                            && fractivity_entries[a].0.allowed_starts.contains(&cur_start)
                        {
                            let v = all_vars.add(variable().binary().initial(0));
                            obj.add_mul(1.0, v);
                            res = Some(v);
                        }
                    }
                }
                xs.push(res);
            }
            xa.push(xs);
        }

        x.push(xa);
    }

    let mut problem = all_vars
        .optimise(ObjectiveDirection::Maximisation, obj)
        .using(default_solver);
    // Distribute max once and only in legal rooms and starts
    for a in 0..fractivity_entries.len() {
        let mut constr = Expression::default();
        for s in 0..num_starts {
            for r in 0..all_rooms.len() {
                if x[a][s as usize][r].is_none() {
                    continue;
                }
                constr.add_mul(1.0, x[a][s as usize][r].unwrap());
            }
        }
        problem.add_constraint(constraint!(constr <= 1));
    }

    for t in 0..num_starts {
        let cur_time = first_start + TIME_RESOLUTION * t;
        // Only one fractivity per room and time t
        for r in 0..all_rooms.len() {
            let mut constr = Expression::default();
            let mut add = 0;
            for a in 0..fractivity_entries.len() {
                for s in 0..num_starts {
                    let cur_start = first_start + TIME_RESOLUTION * s;
                    if occupies(&fractivity_entries[a].0, cur_start, cur_time)
                        && x[a][s as usize][r].is_some()
                    {
                        constr.add_mul(1.0, x[a][s as usize][r].unwrap());
                        add += 1;
                    }
                }
            }
            if add <= 1 {
                continue;
            }
            problem.add_constraint(constraint!(constr <= 1));
        }
        // Only one fractivity per instructor and time t
        for i in 0..all_instructors.len() {
            let mut constr = Expression::default();
            let mut add = 0;
            for a in 0..fractivity_entries.len() {
                if fractivity_entries[a]
                    .0
                    .instructor_extension_uuids
                    .contains(&all_instructors[i])
                {
                    for r in 0..all_rooms.len() {
                        if !fractivity_entries[a]
                            .0
                            .allowed_rooms
                            .contains(&all_rooms[r].id)
                        {
                            continue;
                        }
                        for s in 0..num_starts {
                            let cur_start = first_start + TIME_RESOLUTION * s;
                            if !fractivity_entries[a].0.allowed_starts.contains(&cur_start) {
                                continue;
                            }
                            if occupies(&fractivity_entries[a].0, cur_start, cur_time)
                                && x[a][s as usize][r].is_some()
                            {
                                constr.add_mul(1.0, x[a][s as usize][r].unwrap());
                                add += 1;
                            }
                        }
                    }
                }
            }
            if add <= 1 {
                continue;
            }
            problem.add_constraint(constraint!(constr <= 1));
        }
        let mut constr = Expression::default();
        for a in 0..fractivity_entries.len() {
            for r in 0..all_rooms.len() {
                if !fractivity_entries[a]
                    .0
                    .allowed_rooms
                    .contains(&all_rooms[r].id)
                {
                    continue;
                }
                for s in 0..num_starts {
                    let cur_start = first_start + TIME_RESOLUTION * s;
                    if !fractivity_entries[a].0.allowed_starts.contains(&cur_start) {
                        continue;
                    }
                    if occupies(&fractivity_entries[a].0, cur_start, cur_time)
                        && x[a][s as usize][r].is_some()
                    {
                        constr.add_mul(1.0, x[a][s as usize][r].unwrap());
                    }
                }
            }
        }
    }
    match problem.solve() {
        Ok(solution) => {
            let mut err_undistributed = Vec::new();

            for a in 0..fractivity_entries.len() {
                let mut fract_distributed = false;
                for s in 0..num_starts {
                    for r in 0..all_rooms.len() {
                        if x[a][s as usize][r].is_some()
                            && solution.value(x[a][s as usize][r].unwrap()) > 0.0
                        {
                            fract_distributed = true;
                        }
                    }
                }
                if !fract_distributed {
                    err_undistributed.push(fractivity_entries[a].0.clone());
                }
            }

            Ok(err_undistributed)
        }
        Err(_) => {
            debug!("Tried solving LP to find evil fracties but the solver could not do it!");
            Err("Could not solve undistributable LP".to_string())
        },
    }
}
