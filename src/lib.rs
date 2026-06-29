use std::{cmp::min, collections::HashSet, u32};

use chrono::NaiveDate;
use rand::distr;
use rand_distr::num_traits::ToPrimitive;
use uuid::Uuid;

const TIME_RESOLUTION: i32 = 15;
const MAX_INSTRUCTORS: usize = 32;
const MAX_ROOMS: usize = 32;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FractivityDistribution {
    pub id: Uuid,
    pub instructors: Vec<usize>,
    pub visitors: Vec<usize>,
    pub allowed_starts: u128,
    pub allowed_rooms: Vec<usize>,
    pub duration: u32,
}

fn first_start(fracivity: &FractivityDistribution) -> u32 {
    fracivity.allowed_starts.trailing_zeros()
}

fn last_start(fracivity: &FractivityDistribution) -> u32 {
    127 - fracivity.allowed_starts.leading_zeros()
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

    let mut allowed_starts = 0;
    for start in &fracivity.allowed_starts {
        allowed_starts |= 1u128
            << ((start - earliest_start) / TIME_RESOLUTION
                - (fracivity.preparation_time + TIME_RESOLUTION - 1) / TIME_RESOLUTION);
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
        duration: duration.to_u32().unwrap(),
    }
}

pub fn legal_distribution(
    distributed: &Vec<(FractivityEntry, Uuid, i32)>,
    current: &FractivityEntry,
    rooms: &Vec<Uuid>,
    start: i32,
) -> bool {
    let mut legal_rooms = rooms.clone();
    for (entry, dist_id, dist_start) in distributed.iter() {
        if current.start_day != entry.start_day {
            continue;
        }
        if (start - current.preparation_time >= *dist_start - entry.preparation_time
            && start - current.preparation_time
                < *dist_start + entry.duration + entry.follow_up_time)
            || (*dist_start - entry.preparation_time >= start - current.preparation_time
                && *dist_start - entry.preparation_time
                    < start + current.duration + current.follow_up_time)
        {
            let room_idx = legal_rooms.iter().position(|r| r == dist_id);
            if room_idx.is_some() {
                legal_rooms.remove(room_idx.unwrap());
                if legal_rooms.is_empty() {
                    return false;
                }
            }
            let legal = entry
                .instructor_extension_uuids
                .iter()
                .all(|id| !current.instructor_extension_uuids.contains(id));
            if !legal {
                return false;
            }
        }
    }
    !legal_rooms.is_empty()
}

const OVERLAP_PENALTY: u64 = 1;
const VISITOR_PENALTY: u64 = 1;

pub fn score_distribution_increment(
    open_fractivities: &Vec<FractivityDistribution>,
    distributed: &Vec<(usize, usize, u32)>,
    occupied_instructors: &[u128; MAX_INSTRUCTORS],
    concurrent_fractivities: &[u8; 128]
) -> u64 {
    let mut score = 0;
    
    let last_dist = distributed[distributed.len() - 1].0;
    let start = distributed[distributed.len() - 1].2;
    for i in start..(start + open_fractivities[last_dist].duration) {
        score = std::cmp::max(concurrent_fractivities[i.to_usize().unwrap()].to_u64().unwrap() * OVERLAP_PENALTY, score);
    }
    
    let blocked = blocked_starts(
        1u128 << (distributed[distributed.len() - 1].2 + open_fractivities[last_dist].duration - 1),
        open_fractivities[last_dist].duration,
    );
    let mut current_instructors = occupied_instructors.clone();
    for instructor in open_fractivities[last_dist].visitors.iter() {
        if current_instructors[*instructor] | blocked == 0 {
            current_instructors[*instructor] |= blocked;
        } else {
            score += VISITOR_PENALTY;
        }
    }

    score
}

pub fn score_distribution_increment_start(
    open_fractivities: &Vec<FractivityDistribution>,
    occupied_instructors: &[u128; MAX_INSTRUCTORS],
    concurrent_fractivities: &[u8; 128],
    last_dist: usize,
    start: u32,
) -> u64 {
    let mut score = 0;
    for i in start..(start + open_fractivities[last_dist].duration) {
        score = std::cmp::max(concurrent_fractivities[i.to_usize().unwrap()].to_u64().unwrap() * OVERLAP_PENALTY, score);
    }
    
    let blocked = blocked_starts(
        1u128 << (start + open_fractivities[last_dist].duration - 1),
        open_fractivities[last_dist].duration,
    );
    let mut current_instructors = occupied_instructors.clone();
    for instructor in open_fractivities[last_dist].visitors.iter() {
        if current_instructors[*instructor] | blocked == 0 {
            current_instructors[*instructor] |= blocked;
        } else {
            score += VISITOR_PENALTY;
        }
    }

    score
}

pub fn distribute(
    fractivity_entries: &Vec<FractivityEntry>,
    all_rooms: &Vec<FractivityRoom>,
) -> Result<Vec<(FractivityEntry, Uuid, i32)>, FractivityEntry> {
    let mut instructors = HashSet::new();
    let mut first_start = i32::MAX;

    for entry in fractivity_entries {
        if entry.allowed_rooms.is_empty() || entry.allowed_starts.is_empty() {
            return Err(entry.clone());
        }
        for instructor in &entry.instructor_extension_uuids {
            instructors.insert(*instructor);
        }
        first_start = min(
            first_start,
            entry.allowed_starts[0]
                - ((entry.preparation_time + TIME_RESOLUTION - 1) / TIME_RESOLUTION)
                    * TIME_RESOLUTION,
        );
    }

    let all_instructors: Vec<Uuid> = instructors.into_iter().collect();
    assert!(all_instructors.len() <= MAX_INSTRUCTORS);
    assert!(all_rooms.len() <= MAX_ROOMS);
    let fractivities: Vec<FractivityDistribution> = fractivity_entries
        .clone()
        .iter()
        .map(|frac| convert_fractivity(&frac, all_rooms, &all_instructors, first_start))
        .collect();
    let mut instructor_fractivity_map: [Vec<usize>; MAX_INSTRUCTORS] = Default::default();
    for i in 0..fractivities.len() {
        for instructor in &fractivities[i].instructors {
            instructor_fractivity_map[*instructor].push(i);
        }
    }
    let mut distribution = Vec::new();
    let result = distribute_recursive(
        &fractivities,
        &mut 0,
        &mut Vec::new(),
        &mut vec![0; fractivities.len()],
        &mut [0; MAX_INSTRUCTORS],
        &mut [0; MAX_ROOMS],
        0,
        &mut [0; 128],
        &mut None,
        &mut distribution,
        &instructor_fractivity_map
    );

    match result {
        Ok(()) => {
            return Ok(distribution
                .iter()
                .map(|(frac, room, start)| {
                    let conv_frac = fractivity_entries[*frac].clone();
                    let conv_start = (start.to_i32().unwrap() * TIME_RESOLUTION)
                        + first_start
                        + ((conv_frac.preparation_time.to_i32().unwrap() + TIME_RESOLUTION - 1)
                            / TIME_RESOLUTION)
                            * TIME_RESOLUTION;
                    (
                        conv_frac,
                        all_rooms[room.to_usize().unwrap()].id,
                        conv_start,
                    )
                })
                .collect())
        }
        Err(fractivity) => return Err(fractivity_entries[fractivity].clone()),
    }
}

pub fn fractivity_weight(fracivity: &FractivityDistribution, allowed_starts: u128) -> u32 {
    allowed_starts.count_ones() * fracivity.allowed_rooms.len().to_u32().unwrap_or(0)
}

fn blocked_starts(occupied: u128, duration: u32) -> u128 {
    let mut result = 0;
    for i in 0..duration {
        result |= occupied >> i;
    }
    result
}


fn blocked_and_concurrent(occupied: u128, duration: u32, start: u32, concurrent_fractivities: &mut [u8; 128]) -> u128 {
    let mut result = 0;
    for i in 0..duration {
        result |= occupied >> i;
        concurrent_fractivities[(start + i).to_usize().unwrap()] +=1;
    }
    result
}

fn blocked_and_concurrent_sub(occupied: u128, duration: u32, start: u32, concurrent_fractivities: &mut [u8; 128]) -> u128 {
    let mut result = 0;
    for i in 0..duration {
        result |= occupied >> i;
        concurrent_fractivities[(start + i).to_usize().unwrap()] -=1;
    }
    result
}

fn set_fractivity_occupation(
    fractivity: &FractivityDistribution,
    start: u32,
    room: usize,
    occupied_instructors: &mut [u128; MAX_INSTRUCTORS],
    occupied_rooms: &mut [u128; MAX_ROOMS],
    concurrent_fractivities: &mut [u8; 128],
) {
    let blocked = blocked_and_concurrent(
        1u128 << (start + fractivity.duration - 1),
        fractivity.duration,
        start,
        concurrent_fractivities
    );
    for instructor in fractivity.instructors.iter() {
        occupied_instructors[*instructor] |= blocked;
    }
    occupied_rooms[room] |= blocked;
}

fn unset_fractivity_occupation(
    fractivity: &FractivityDistribution,
    start: u32,
    room: usize,
    occupied_instructors: &mut [u128; MAX_INSTRUCTORS],
    occupied_rooms: &mut [u128; MAX_ROOMS],
    concurrent_fractivities: &mut [u8; 128]
) {
    let blocked = !blocked_and_concurrent_sub(
        1u128 << (start + fractivity.duration - 1),
        fractivity.duration,
        start,
        concurrent_fractivities
    );
    for instructor in fractivity.instructors.iter() {
        occupied_instructors[*instructor] &= blocked;
    }
    occupied_rooms[room] &= blocked;
}

pub fn distribute_recursive(
    all_fractivities: &Vec<FractivityDistribution>,
    assigned: &mut u128,
    distributed: &mut Vec<(usize, usize, u32)>,
    distributed_index: &mut Vec<usize>,
    occupied_instructors: &mut [u128; MAX_INSTRUCTORS],
    occupied_rooms: &mut [u128; MAX_ROOMS],
    current_score: u64,
    concurrent_fractivities: &mut [u8; 128],
    best_score: &mut Option<u64>,
    best_solution: &mut Vec<(usize, usize, u32)>,
    instructor_fractivity_map: &[Vec<usize>; MAX_INSTRUCTORS]
) -> Result<(), usize> {
    if all_fractivities.len() == distributed.len() {
        if best_score.is_none() || best_score.unwrap() > current_score {
            *best_score = Some(current_score);
            *best_solution = distributed.clone();
        }
        return Ok(());
    }
    if best_score.is_some() && current_score >= best_score.unwrap() {
        return Err(0);
    }
    let mut frac_to_distribute_index = all_fractivities.len();
    let mut starts = 0;
    let mut best_weight = u32::MAX;

    for i in 0..all_fractivities.len() {
        if *assigned & (1u128 << i) != 0 {
            continue;
        }
        let mut allowed_starts = all_fractivities[i].allowed_starts;
        for instructor in all_fractivities[i].instructors.iter() {
            allowed_starts &= !blocked_starts(
                occupied_instructors[*instructor],
                all_fractivities[i].duration,
            );
        }

        let mut rooms_blocked = all_fractivities[i].allowed_starts;
        for room in all_fractivities[i].allowed_rooms.iter() {
            rooms_blocked &= blocked_starts(occupied_rooms[*room], all_fractivities[i].duration);
        }
        allowed_starts &= !rooms_blocked;
        if allowed_starts == 0 {
            return Err(i);
        }
        let weight = fractivity_weight(&all_fractivities[i], allowed_starts);
        if weight < best_weight {
            best_weight = weight;
            starts = allowed_starts;
            frac_to_distribute_index = i;
        }
    }
    let mut start_vec: Vec<(u32, u64)> = Vec::new();
    let mut cur_starts = starts;
    while cur_starts > 0 {
        let start = cur_starts.trailing_zeros();
        start_vec.push((start, score_distribution_increment_start(all_fractivities,
                            occupied_instructors,
                            concurrent_fractivities, frac_to_distribute_index, start)));
        cur_starts &= !(1u128 << start);
    }
    start_vec.sort_unstable_by(|(_,v1), (_,v2)| v1.cmp(v2));

    let mut result = Err(frac_to_distribute_index);
    for room in all_fractivities[frac_to_distribute_index]
        .allowed_rooms
        .iter()
    {
        let current_starts = starts
            & !blocked_starts(
                occupied_rooms[*room],
                all_fractivities[frac_to_distribute_index].duration,
            );
        for &(start, _) in &start_vec {
            if current_starts & (1 << start) == 0 {
                continue;
            }
            distributed.push((frac_to_distribute_index, *room, start));
            distributed_index[frac_to_distribute_index] = distributed.len() - 1;
            *assigned |= 1u128 << frac_to_distribute_index;
            set_fractivity_occupation(
                &all_fractivities[frac_to_distribute_index],
                start,
                *room,
                occupied_instructors,
                occupied_rooms,
                concurrent_fractivities,
            );
            let mut all_meet_demand = true;
            for instructor in all_fractivities[frac_to_distribute_index]
                .instructors
                .iter()
            {
                all_meet_demand &=
                    check_specific_instructor_demand(all_fractivities, distributed,
    distributed_index, *instructor, assigned, instructor_fractivity_map);
                if !all_meet_demand {
                    break;
                }
            }
            if all_meet_demand {
                let score_pre = best_score.clone();
                let current_dist = distribute_recursive(
                    all_fractivities,
                    assigned,
                    distributed,
                    distributed_index,
                    occupied_instructors,
                    occupied_rooms,
                    current_score
                        + score_distribution_increment(
                            all_fractivities,
                            distributed,
                            occupied_instructors,
                            concurrent_fractivities
                        ),
                        concurrent_fractivities,
                    best_score,
                    best_solution,
    instructor_fractivity_map
                );
                match current_dist {
                    Ok(()) => {
                        if result.is_err()
                            || score_pre.is_none()
                            || (best_score.is_some() && best_score.unwrap() < score_pre.unwrap())
                        {
                            result = Ok(());
                        }
                    }
                    Err(res) => {
                        if result.is_err() {
                            result = Err(res);
                        }
                    }
                }
            }
            distributed.pop();
            unset_fractivity_occupation(
                &all_fractivities[frac_to_distribute_index],
                start,
                *room,
                occupied_instructors,
                occupied_rooms,
                concurrent_fractivities
            );
            *assigned &= !(1u128 << frac_to_distribute_index);
        }
    }
    result
}

pub fn check_specific_instructor_demand(
    all_fractivities: &Vec<FractivityDistribution>,
    distributed: &Vec<(usize, usize, u32)>,
    distributed_index: &mut Vec<usize>,
    instructor: usize,
    assigned: &u128,
    instructor_fractivity_map: &[Vec<usize>; MAX_INSTRUCTORS]
) -> bool {
    let mut instructor_open: Vec<usize> = Vec::new();
    let mut instructor_distributed: Vec<&(usize, usize, u32)> = Vec::new();

    for frac in &instructor_fractivity_map[instructor] {
        if assigned & 1 << frac == 0 {
            instructor_open.push(*frac);
        } else {
            instructor_distributed.push(&distributed[distributed_index[*frac]]);
        }
    }

    if instructor_open.is_empty() {
        return true;
    }

    let mut all_starts: u128 = 0;
    let mut all_deadlines: u128 = 0;
    for frac in &instructor_open {
        all_starts |= 1 << first_start(&all_fractivities[*frac]);
        all_deadlines |=  1 << (last_start(&all_fractivities[*frac]) + all_fractivities[*frac].duration);
    }

    while all_starts > 0 {
        let start =  all_starts.trailing_zeros();
        let mut cur_deadlines = all_deadlines;
        while cur_deadlines > 0 {
            let deadline = cur_deadlines.trailing_zeros();
            if deadline <= start {
                continue;
            }
            if !check_instructor_demand(
                all_fractivities,
                &instructor_open,
                &instructor_distributed,
                (start, deadline),
            ) {
                return false;
            }
            cur_deadlines &= !(1 << deadline);
        }
        all_starts &= !(1 << start);
    }

    true
}

// Assume all fractivities share instuctors
pub fn check_instructor_demand(
    all_fractivities: &Vec<FractivityDistribution>,
    open_fractivities: &Vec<usize>,
    distributed: &Vec<&(usize, usize, u32)>,
    interval: (u32, u32),
) -> bool {
    let capacity = interval.1 - interval.0;
    let mut demand = 0;

    for (entry, _, start) in distributed {
        demand += if *start >= interval.0 && *start + all_fractivities[*entry].duration < interval.1
        {
            all_fractivities[*entry].duration
        } else {
            0
        };
    }

    for entry in open_fractivities {
        demand += if first_start(&all_fractivities[*entry]) >= interval.0
            && last_start(&all_fractivities[*entry]) + all_fractivities[*entry].duration
                < interval.1
        {
            all_fractivities[*entry].duration
        } else {
            0
        };
    }
    demand <= capacity
}
