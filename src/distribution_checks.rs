pub mod checks {
    use good_lp::{
        constraint, solvers::highs, variable, variables, Expression, ObjectiveDirection,
        ProblemVariables, Solution, SolverModel, Variable, WithMipGap,
    };

    use std::{cmp::max, cmp::min, collections::HashSet};
    use uuid::Uuid;

    use crate::{FractivityEntry, FractivityRoom};

    const TIME_RESOLUTION: i32 = 15;

    fn occupies(a: &FractivityEntry, s: i32, t: i32) -> bool {
        return t >= s - a.preparation_time && t < s + a.duration + a.follow_up_time;
    }

    fn overlap(a1: &FractivityEntry, s1: i32, a2: &FractivityEntry, s2: i32) -> bool {
        occupies(a1, s1, s2 - a2.preparation_time) || occupies(a2, s2, s1 - a1.preparation_time)
    }

    pub fn reference_distribution(
        fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
        all_rooms: Vec<FractivityRoom>,
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
    ) -> Vec<Vec<Vec<Option<Variable>>>> {
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
                                let v = all_vars.add(variable().binary());
                                objective.add_mul(
                                    (fractivity_entries.len()
                                        * fractivity_entries.len()
                                        * all_instructors.len())
                                        as f64,
                                    v,
                                );
                                res = Some(v);
                            }
                        }
                        None => {
                            if fractivity_entries[a].0.allowed_rooms.contains(&r.id)
                                && fractivity_entries[a].0.allowed_starts.contains(&cur_start)
                            {
                                let v = all_vars.add(variable().binary());
                                objective.add_mul(
                                    (fractivity_entries.len() * all_instructors.len()) as f64
                                        + r.priority as f64
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

        x
    }

    fn create_optimizer_variables(
        fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
        all_rooms: &Vec<FractivityRoom>,
        all_instructors: &Vec<Uuid>,
        first_start: i32,
        num_starts: i32,
        all_vars: &mut ProblemVariables,
        objective: &mut Expression,
    ) -> (Vec<Vec<Variable>>, Vec<Vec<Vec<Option<Variable>>>>) {
        let mut b = Vec::new();
        let mut v = Vec::new();
        let max_parallel = fractivity_entries.len().min(all_rooms.len());

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
                        let var = all_vars.add(variable().binary());
                        objective.add_mul(1.0, var);
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
                let btk = all_vars.add(variable().binary());
                objective.add_mul(
                    -0.5 * ((k + 1) as f64)
                        / (fractivity_entries.len() * all_instructors.len() * max_parallel) as f64,
                    btk,
                );
                bt.push(btk);
            }
            b.push(bt);
        }

        (b, v)
    }

    fn lp_distribution(
        fractivity_entries: &Vec<(FractivityEntry, Option<(Uuid, i32)>)>,
        all_rooms: &Vec<FractivityRoom>,
        all_instructors: &Vec<Uuid>,
        first_start: i32,
        num_starts: i32,
    ) -> Result<Result<Vec<(usize, Uuid, i32)>, Vec<FractivityEntry>>, String> {
        let mut all_vars = variables!();
        let mut obj = Expression::default();
        let x = create_fractivity_variables(
            fractivity_entries,
            all_rooms,
            all_instructors,
            first_start,
            num_starts,
            &mut all_vars,
            &mut obj,
        );
        let (b, v) = create_optimizer_variables(
            fractivity_entries,
            all_rooms,
            all_instructors,
            first_start,
            num_starts,
            &mut all_vars,
            &mut obj,
        );

        let mut problem = match all_vars
            .optimise(ObjectiveDirection::Maximisation, obj)
            .using(highs::highs)
            //.with_time_limit(1)
            .with_mip_gap(
                0.5 / (fractivity_entries.len() * fractivity_entries.len() * all_instructors.len())
                    as f32,
            ) {
            Ok(o) => o,
            Err(e) => return Err(e.to_string()),
        };
        //problem.set_parameter("log", "0");

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
            problem.add_constraint(constraint!(constr.clone() <= 1));
            for i in 0..all_instructors.len() {
                if !fractivity_entries[a]
                    .0
                    .visitor_extension_uuids
                    .contains(&all_instructors[i])
                {
                    continue;
                }
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
                    problem.add_constraint(constraint!(
                        v[a][i][s as usize].unwrap() <= constr.clone()
                    ));
                }
            }
        }

        let max_parallel = fractivity_entries.len().min(all_rooms.len());
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
                problem.add_constraint(constraint!(constr.clone() <= 1));
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
                problem.add_constraint(constraint!(constr.clone() <= 1));
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
                    constr.clone() <= b[t as usize][k] * max_parallel as f64 + k as f64
                ));
            }
        }

        match problem.solve() {
            Ok(solution) => {
                let mut result_distributed = Vec::new();
                let mut err_undistributed = Vec::new();

                for a in 0..fractivity_entries.len() {
                    let mut fract_distributed = false;
                    for s in 0..num_starts {
                        let cur_start = first_start + TIME_RESOLUTION * s;
                        for r in 0..all_rooms.len() {
                            if x[a][s as usize][r].is_some()
                                && solution.value(x[a][s as usize][r].unwrap()) > 0.0
                            {
                                fract_distributed = true;
                                result_distributed.push((a, all_rooms[r].id, cur_start));
                            }
                        }
                    }
                    if !fract_distributed {
                        err_undistributed.push(fractivity_entries[a].0.clone());
                    }
                }

                if !err_undistributed.is_empty() {
                    Ok(Err(err_undistributed))
                } else {
                    Ok(Ok(result_distributed))
                }
            }
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn check_distribution(distribution: &Vec<(FractivityEntry, Uuid, i32)>) -> bool {
        for a in 0..distribution.len() {
            for b in (a + 1)..distribution.len() {
                let (entry1, room1, start1) = &distribution[a];
                let (entry2, room2, start2) = &distribution[b];

                if !overlap(entry1, *start1, entry2, *start2) {
                    continue;
                }

                if room1 == room2 {
                    println!(
                        "fractivity {:?} start {:?} duration {:?} preparation {:?} follow_up {:?}",
                        entry1.id,
                        start1,
                        entry1.duration,
                        entry1.preparation_time,
                        entry1.follow_up_time
                    );
                    println!(
                        "fractivity {:?} start {:?} duration {:?} preparation {:?} follow_up {:?}",
                        entry2.id,
                        start2,
                        entry2.duration,
                        entry2.preparation_time,
                        entry2.follow_up_time
                    );
                    println!("Rooms overlap!");
                    return false;
                }
                for i1 in &entry1.instructor_extension_uuids {
                    if entry2.instructor_extension_uuids.contains(i1) {
                        println!("fractivity {:?} start {:?} duration {:?} preparation {:?} follow_up {:?}", entry1.id, start1, entry1.duration, entry1.preparation_time, entry1.follow_up_time);
                        println!("fractivity {:?} start {:?} duration {:?} preparation {:?} follow_up {:?}", entry2.id, start2, entry2.duration, entry2.preparation_time, entry2.follow_up_time);

                        println!(
                            "instructors overlap! {:?}, {:?}",
                            entry1.instructor_extension_uuids, entry2.instructor_extension_uuids
                        );
                        return false;
                    }
                }
            }
        }

        true
    }
}
