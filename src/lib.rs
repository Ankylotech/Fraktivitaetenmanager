use std::{cmp::max, cmp::min, collections::HashSet, u32};

use chrono::NaiveDate;
use uuid::Uuid;

use good_lp::{
    constraint, variable, variables, Expression, ObjectiveDirection, Solution, SolverModel,
    WithTimeLimit,
};

// Fields that should not matter are intentionaly removed
#[derive(Clone, Debug)]
pub struct FractivityRoom {
    pub id: Uuid,
    pub priority: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FractivityEntry {
    pub id: Uuid,
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
const MAX_INSTRUCTORS: usize = 32;
const MAX_ROOMS: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FractivityDistribution {
    pub id: Uuid,
    pub instructors: Vec<usize>,
    pub visitors: Vec<usize>,
    pub allowed_starts: Vec<u32>,
    pub allowed_rooms: Vec<usize>,
    pub duration: u32,
}

fn convert_fractivity(
    fracivity: &FractivityEntry,
    all_rooms: &Vec<FractivityRoom>,
    all_instructors: &Vec<Uuid>,
    earliest_start: i32,
) -> FractivityDistribution {
    let mut instructors = Vec::new();
    for instructor in &fracivity.instructor_extension_uuids {
        instructors.push(
            all_instructors
                .iter()
                .position(|i| i == instructor)
                .unwrap(),
        );
    }

    let mut visitors = Vec::new();
    for instructor in &fracivity.visitor_extension_uuids {
        visitors.push(
            all_instructors
                .iter()
                .position(|i| i == instructor)
                .unwrap(),
        );
    }

    let mut allowed_starts = Vec::new();
    for start in &fracivity.allowed_starts {
        allowed_starts.push(
            ((start - earliest_start) / TIME_RESOLUTION
                - (fracivity.preparation_time + TIME_RESOLUTION - 1) / TIME_RESOLUTION)
                as u32,
        );
    }

    let mut allowed_rooms = Vec::new();
    for room in &fracivity.allowed_rooms {
        allowed_rooms.push(all_rooms.iter().position(|i| i.id == *room).unwrap());
    }

    let duration = (fracivity.duration + fracivity.follow_up_time + TIME_RESOLUTION - 1)
        / TIME_RESOLUTION
        + (fracivity.preparation_time + TIME_RESOLUTION - 1) / TIME_RESOLUTION;

    FractivityDistribution {
        id: fracivity.id,
        instructors,
        visitors,
        allowed_starts,
        allowed_rooms,
        duration: duration as u32,
    }
}

fn occupies(a: &FractivityDistribution, s: u32, t: u32) -> bool {
    return t >= s && t < s + a.duration;
}

fn reconvert_start(s: usize, a: &FractivityEntry, first_start: i32) -> i32 {
    (s as i32 * TIME_RESOLUTION)
        + first_start
        + ((a.preparation_time + TIME_RESOLUTION - 1) / TIME_RESOLUTION) * TIME_RESOLUTION
}

pub fn distribute(
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
    all_rooms: &Vec<FractivityRoom>,
) -> Result<Vec<(FractivityEntry, Uuid, i32)>, Vec<FractivityEntry>> {
    let mut initial;
    match test_distributable(fractivity_entries, all_rooms) {
        Ok(v) => initial = v,
        Err(v) => return Err(v),
    }
    let mut instructors = HashSet::new();
    let mut first_start = i32::MAX;
    let mut last_start = i32::MIN;

    for entry in fractivity_entries {
        if entry.0.allowed_rooms.is_empty() || entry.0.allowed_starts.is_empty() {
            return Err(vec![entry.0.clone()]);
        }
        for instructor in &entry.0.instructor_extension_uuids {
            instructors.insert(*instructor);
        }
        first_start = min(
            first_start,
            entry.0.allowed_starts[0]
                - ((entry.0.preparation_time + TIME_RESOLUTION - 1) / TIME_RESOLUTION)
                    * TIME_RESOLUTION,
        );

        last_start = max(
            last_start,
            entry.0.allowed_starts[entry.0.allowed_starts.len() - 1]
                - ((entry.0.preparation_time + TIME_RESOLUTION - 1) / TIME_RESOLUTION)
                    * TIME_RESOLUTION,
        );
    }

    let num_starts = ((last_start - first_start) / TIME_RESOLUTION) as usize + 1;
    let all_instructors: Vec<Uuid> = instructors.into_iter().collect();
    assert!(all_instructors.len() <= MAX_INSTRUCTORS);
    assert!(all_rooms.len() <= MAX_ROOMS);
    let fractivities: Vec<FractivityDistribution> = fractivity_entries
        .clone()
        .iter()
        .map(|frac| convert_fractivity(&frac.0, all_rooms, &all_instructors, first_start))
        .collect();

    let mut all_vars = variables!();
    let mut obj = Expression::default();

    let mut x = Vec::new();
    for a in 0..fractivities.len() {
        let mut xa = Vec::new();
        for s in 0..num_starts {
            let mut xs = Vec::new();
            for r in 0..all_rooms.len() {
                let mut res = None;
                match fractivity_entries[a].1 {
                    Some((id, start)) => {
                        if all_rooms[r].id == id
                            && reconvert_start(s, &fractivity_entries[a].0, first_start) == start
                        {
                            let v = all_vars.add(variable().binary().initial(1));
                            obj.add_mul(fractivities.len() as f64, v);
                            res = Some(v);
                        }
                    }
                    None => {
                        if fractivities[a].allowed_rooms.contains(&r)
                            && fractivities[a].allowed_starts.contains(&(s as u32))
                        {
                            let v = all_vars.add(
                                variable().binary().initial(if initial[a] == (r, s) {
                                    1
                                } else {
                                    0
                                }),
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

    let mut v = Vec::new();
    for a in 0..fractivities.len() {
        let mut va = Vec::new();
        for i in 0..all_instructors.len() {
            let mut vv = Vec::new();
            for s in 0..num_starts {
                let mut res = None;
                if fractivities[a].visitors.contains(&i)
                    && fractivities[a].allowed_starts.contains(&(s as u32))
                {
                    let var = all_vars.add(variable().binary().initial(0));
                    obj.add_mul(0.5 / fractivities.len() as f64, var);
                    res = Some(var);
                }

                vv.push(res);
            }
            va.push(vv);
        }
        v.push(va);
    }

    let mut b = Vec::new();
    for _ in 0..num_starts {
        let mut bt = Vec::new();
        for k in 0..fractivities.len() {
            let btk = all_vars.add(variable().binary().initial(1));
            obj.add_mul(
                -0.5 * (k as f64)
                    / (fractivities.len()
                        * fractivities.len()
                        * fractivities.len()
                        * all_instructors.len()) as f64,
                btk,
            );
            bt.push(btk);
        }
        b.push(bt);
    }

    let mut problem = all_vars
        .optimise(ObjectiveDirection::Maximisation, obj)
        .using(good_lp::highs)
        .with_time_limit(1);

    // Distribute max once and only in legal rooms and starts
    for a in 0..fractivities.len() {
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
                        constr.add_mul(1.0, x[a][s][r].unwrap());
                    }
                }
                problem.add_constraint(constraint!(v[a][i][s as usize].unwrap() <= constr));
            }
        }
    }

    for t in 0..num_starts {
        // Only one fractivity per room and time t
        for r in 0..all_rooms.len() {
            let mut constr = Expression::default();
            let mut added = false;
            for a in 0..fractivities.len() {
                for s in 0..num_starts {
                    if occupies(&fractivities[a], s as u32, t as u32)
                        && x[a][s as usize][r].is_some()
                    {
                        constr.add_mul(1.0, x[a][s as usize][r].unwrap());
                        added = true;
                    }
                }
            }
            if !added {
                continue;
            }
            problem.add_constraint(constraint!(constr <= 1));
        }
        // Only one fractivity per instructor and time t
        for i in 0..all_instructors.len() {
            let mut constr = Expression::default();
            let mut added = false;
            for a in 0..fractivities.len() {
                if fractivities[a].instructors.contains(&i) {
                    for r in 0..all_rooms.len() {
                        if !fractivities[a].allowed_rooms.contains(&r) {
                            continue;
                        }
                        for s in 0..num_starts {
                            if !fractivities[a].allowed_starts.contains(&(s as u32)) {
                                continue;
                            }
                            if occupies(&fractivities[a], s as u32, t as u32)
                                && x[a][s as usize][r].is_some()
                            {
                                constr.add_mul(1.0, x[a][s as usize][r].unwrap());
                                added = true;
                            }
                        }
                    }
                }
                if fractivities[a].visitors.contains(&i) {
                    for s in 0..num_starts {
                        if !fractivities[a].allowed_starts.contains(&(s as u32)) {
                            continue;
                        }
                        if occupies(&fractivities[a], s as u32, t as u32)
                            && v[a][i][s as usize].is_some()
                        {
                            constr.add_mul(1.0, v[a][i][s as usize].unwrap());
                            added = true;
                        }
                    }
                }
            }
            if !added {
                continue;
            }
            problem.add_constraint(constraint!(constr <= 1));
        }
        let mut constr = Expression::default();
        for a in 0..fractivities.len() {
            for r in 0..all_rooms.len() {
                if !fractivities[a].allowed_rooms.contains(&r) {
                    continue;
                }
                for s in 0..num_starts {
                    if !fractivities[a].allowed_starts.contains(&(s as u32)) {
                        continue;
                    }
                    if occupies(&fractivities[a], s as u32, t as u32)
                        && x[a][s as usize][r].is_some()
                    {
                        constr.add_mul(1.0, x[a][s as usize][r].unwrap());
                    }
                }
            }
        }
        for k in 0..fractivities.len() {
            problem.add_constraint(constraint!(
                constr.clone() <= b[t][k] * fractivities.len() as f64 + (k as f64 - 1.0)
            ));
        }
    }

    let solution = problem.solve().unwrap();
    let mut result_distributed = Vec::new();
    let mut err_undistributed = Vec::new();

    for a in 0..fractivities.len() {
        let mut fract_distributed = false;
        for s in 0..num_starts {
            for r in 0..all_rooms.len() {
                if x[a][s as usize][r].is_some()
                    && solution.value(x[a][s as usize][r].unwrap()) > 0.0
                {
                    fract_distributed = true;
                    result_distributed.push((
                        fractivity_entries[a].0.clone(),
                        all_rooms[r].id,
                        reconvert_start(s, &fractivity_entries[a].0, first_start),
                    ));
                }
            }
        }
        if !fract_distributed {
            err_undistributed.push(fractivity_entries[a].0.clone());
        }
    }

    if err_undistributed.len() > 0 {
        Err(err_undistributed)
    } else {
        Ok(result_distributed)
    }
}

pub fn test_distributable(
    fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
    all_rooms: &Vec<FractivityRoom>,
) -> Result<Vec<(usize, usize)>, Vec<FractivityEntry>> {
    let mut instructors = HashSet::new();
    let mut first_start = i32::MAX;
    let mut last_start = i32::MIN;

    for entry in fractivity_entries {
        if entry.0.allowed_rooms.is_empty() || entry.0.allowed_starts.is_empty() {
            return Err(vec![entry.0.clone()]);
        }
        for instructor in &entry.0.instructor_extension_uuids {
            instructors.insert(*instructor);
        }
        first_start = min(
            first_start,
            entry.0.allowed_starts[0]
                - ((entry.0.preparation_time + TIME_RESOLUTION - 1) / TIME_RESOLUTION)
                    * TIME_RESOLUTION,
        );

        last_start = max(
            last_start,
            entry.0.allowed_starts[entry.0.allowed_starts.len() - 1]
                - ((entry.0.preparation_time + TIME_RESOLUTION - 1) / TIME_RESOLUTION)
                    * TIME_RESOLUTION,
        );
    }

    let num_starts = ((last_start - first_start) / TIME_RESOLUTION) as usize + 1;
    let all_instructors: Vec<Uuid> = instructors.into_iter().collect();
    assert!(all_instructors.len() <= MAX_INSTRUCTORS);
    assert!(all_rooms.len() <= MAX_ROOMS);
    let fractivities: Vec<FractivityDistribution> = fractivity_entries
        .clone()
        .iter()
        .map(|frac| convert_fractivity(&frac.0, all_rooms, &all_instructors, first_start))
        .collect();

    let mut all_vars = variables!();
    let mut obj = Expression::default();

    let mut x = Vec::new();
    for a in 0..fractivities.len() {
        let mut xa = Vec::new();
        for s in 0..num_starts {
            let mut xs = Vec::new();
            for r in 0..all_rooms.len() {
                let mut res = None;
                match fractivity_entries[a].1 {
                    Some((id, start)) => {
                        if all_rooms[r].id == id
                            && reconvert_start(s, &fractivity_entries[a].0, first_start) == start
                        {
                            let v = all_vars.add(variable().binary());
                            obj.add_mul(fractivities.len() as f64, v);
                            res = Some(v);
                        }
                    }
                    None => {
                        if fractivities[a].allowed_rooms.contains(&r)
                            && fractivities[a].allowed_starts.contains(&(s as u32))
                        {
                            let v = all_vars.add(variable().binary());
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
        .using(good_lp::highs);

    // Distribute max once and only in legal rooms and starts
    for a in 0..fractivities.len() {
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
        // Only one fractivity per room and time t
        for r in 0..all_rooms.len() {
            let mut constr = Expression::default();
            let mut added = false;
            for a in 0..fractivities.len() {
                for s in 0..num_starts {
                    if occupies(&fractivities[a], s as u32, t as u32)
                        && x[a][s as usize][r].is_some()
                    {
                        constr.add_mul(1.0, x[a][s as usize][r].unwrap());
                        added = true;
                    }
                }
            }
            if !added {
                continue;
            }
            problem.add_constraint(constraint!(constr <= 1));
        }
        // Only one fractivity per instructor and time t
        for i in 0..all_instructors.len() {
            let mut constr = Expression::default();
            let mut added = false;
            for a in 0..fractivities.len() {
                if fractivities[a].instructors.contains(&i) {
                    for r in 0..all_rooms.len() {
                        if !fractivities[a].allowed_rooms.contains(&r) {
                            continue;
                        }
                        for s in 0..num_starts {
                            if !fractivities[a].allowed_starts.contains(&(s as u32)) {
                                continue;
                            }
                            if occupies(&fractivities[a], s as u32, t as u32)
                                && x[a][s as usize][r].is_some()
                            {
                                constr.add_mul(1.0, x[a][s as usize][r].unwrap());
                                added = true;
                            }
                        }
                    }
                }
            }
            if !added {
                continue;
            }
            problem.add_constraint(constraint!(constr <= 1));
        }
    }

    let solution = problem.solve().unwrap();
    let mut result_distributed = Vec::new();
    let mut err_undistributed = Vec::new();

    for a in 0..fractivities.len() {
        let mut fract_distributed = false;
        for s in 0..num_starts {
            for r in 0..all_rooms.len() {
                if x[a][s as usize][r].is_some()
                    && solution.value(x[a][s as usize][r].unwrap()) > 0.0
                {
                    fract_distributed = true;
                    result_distributed.push((r, s));
                }
            }
        }
        if !fract_distributed {
            err_undistributed.push(fractivity_entries[a].0.clone());
        }
    }

    if err_undistributed.len() > 0 {
        Err(err_undistributed)
    } else {
        Ok(result_distributed)
    }
}
