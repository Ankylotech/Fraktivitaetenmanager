use std::time::Duration;

use rand::{seq, RngExt};
use rand_distr::{num_traits::ToPrimitive, Distribution, Normal};
use uuid::Uuid;
use Fraktivitaeten::{
    distribute,
    FractivityDistributionFailReason::{self, RoomUnavailable},
    FractivityEntry, FractivityRoom,
};

use crate::distribution_checks::checks::{self, check_distribution};

mod distribution_checks;

#[cfg(test)]
pub mod tests {
    use uuid::Uuid;
    use Fraktivitaeten::{distribute, FractivityEntry, FractivityRoom};

    fn rooms() -> Vec<FractivityRoom> {
        [FractivityRoom {
            id: Uuid::from_u128(0),
            priority: 0,
        }]
        .to_vec()
    }

    fn undistributed_entries() -> Vec<(FractivityEntry, Option<(Uuid, i32)>)> {
        [
            (
                FractivityEntry {
                    id: uuid::Uuid::from_u128(0),
                    topic: "0".to_string(),
                    instructor_extension_uuids: vec![Uuid::from_u128(0)],
                    visitor_extension_uuids: vec![Uuid::from_u128(1)],
                    duration: 45,
                    allowed_rooms: vec![Uuid::from_u128(0)],
                    allowed_starts: vec![810, 825, 840, 855, 870],
                    preparation_time: 0,
                    follow_up_time: 0,
                    blocker: false,
                },
                None,
            ),
            (
                FractivityEntry {
                    id: uuid::Uuid::from_u128(1),
                    topic: "1".to_string(),
                    instructor_extension_uuids: vec![Uuid::from_u128(1)],
                    visitor_extension_uuids: Vec::new(),
                    duration: 45,
                    allowed_rooms: vec![Uuid::from_u128(0)],
                    allowed_starts: vec![810],
                    preparation_time: 0,
                    follow_up_time: 0,
                    blocker: false,
                },
                None,
            ),
        ]
        .to_vec()
    }

    fn correct_distribution() -> Vec<(Uuid, i32)> {
        [(Uuid::from_u128(0), 810), (Uuid::from_u128(0), 855)].to_vec()
    }

    fn undistributable_entries() -> Vec<(FractivityEntry, Option<(Uuid, i32)>)> {
        [
            (
                FractivityEntry {
                    id: uuid::Uuid::from_u128(0),
                    topic: "0".to_string(),
                    instructor_extension_uuids: vec![Uuid::from_u128(0)],
                    visitor_extension_uuids: Vec::new(),
                    duration: 30,
                    allowed_rooms: vec![Uuid::from_u128(0)],
                    allowed_starts: vec![0],
                    preparation_time: 0,
                    follow_up_time: 0,
                    blocker: false,
                },
                None,
            ),
            (
                FractivityEntry {
                    id: uuid::Uuid::from_u128(1),
                    topic: "1".to_string(),
                    instructor_extension_uuids: vec![Uuid::from_u128(1)],
                    visitor_extension_uuids: Vec::new(),
                    duration: 30,
                    allowed_rooms: vec![Uuid::from_u128(0)],
                    allowed_starts: vec![0, 15, 30, 45],
                    preparation_time: 5,
                    follow_up_time: 0,
                    blocker: false,
                },
                None,
            ),
            (
                FractivityEntry {
                    id: uuid::Uuid::from_u128(2),
                    topic: "2".to_string(),
                    instructor_extension_uuids: vec![Uuid::from_u128(0), Uuid::from_u128(1)],
                    visitor_extension_uuids: Vec::new(),
                    duration: 30,
                    allowed_rooms: vec![Uuid::from_u128(1)],
                    allowed_starts: vec![0, 15, 30, 45],
                    preparation_time: 5,
                    follow_up_time: 0,
                    blocker: false,
                },
                None,
            ),
            (
                FractivityEntry {
                    id: uuid::Uuid::from_u128(3),
                    topic: "3".to_string(),
                    instructor_extension_uuids: vec![Uuid::from_u128(2)],
                    visitor_extension_uuids: Vec::new(),
                    duration: 30,
                    allowed_rooms: vec![Uuid::from_u128(1)],
                    allowed_starts: vec![0, 15, 30, 45],
                    preparation_time: 5,
                    follow_up_time: 0,
                    blocker: false,
                },
                None,
            ),
        ]
        .to_vec()
    }

    #[test]
    pub fn test_undistributed() {
        let entries = undistributed_entries();
        let result = distribute(&entries.clone(), rooms()).unwrap();
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.len() == entries.len());
        let dist = correct_distribution();
        for i in 0..result.len() {
            let mut found = false;
            println!("{:?} {:?}", result[i].1, result[i].2);
            for j in 0..entries.len() {
                if result[i].0.id == entries[j].0.id {
                    found = true;
                    //assert!(result[i].1 == dist[j].0 && result[i].2 == dist[j].1);
                }
            }
            //assert!(found);
        }
    }

    #[test]
    pub fn test_undistributable() {
        let entries = undistributable_entries();
        let result = distribute(&mut entries.clone(), rooms()).unwrap();
        assert!(result.is_err());
        assert!(!result.unwrap_err().is_empty());
    }
}

fn generate_random_duration() -> i32 {
    let avg_frac_duration = 90.0;
    let min_frac_duration = 30.0;

    let mut rng = rand::rng();
    let normal =
        Normal::new(avg_frac_duration, 30.0).expect("Failed to create normal distribution");
    let mut v: f64 = normal.sample(&mut rng);
    while v < min_frac_duration {
        v = normal.sample(&mut rng);
    }
    (v / 5.0).to_i32().unwrap() * 5
}

fn generate_random_starts(start: i32) -> Vec<i32> {
    let mut rng = rand::rng();
    let amount = rng.random_range(45..120);

    ((start)..(start + amount)).step_by(15).collect()
}

fn generate_random_fractivity_entries(
    rooms: &Vec<FractivityRoom>,
    instuctors: &Vec<Uuid>,
) -> Vec<(FractivityEntry, Option<(Uuid, i32)>)> {
    //let min_fracs = 40;
    let day_duration = 7 * 60;
    let mut result = Vec::new();
    let mut rng = rand::rng();
    for i in (0..day_duration).step_by(120) {
        let mut available_instructor = 0;
        for k in 0..10 {
            let num_instructors =
                rng.random_range(1..(instuctors.len() - available_instructor - (9 - k)));
            let chosen_instructors = (available_instructor
                ..(available_instructor + num_instructors))
                .map(|i| instuctors[i])
                .collect();
            available_instructor += num_instructors;
            let num_visitors =
                rng.random_range(0..(instuctors.len() - available_instructor - (9 - k)));
            let chosen_visitors = (available_instructor..(available_instructor + num_visitors))
                .map(|i| instuctors[i])
                .collect();
            available_instructor += num_visitors;
            let room_idx = rng.random_range(1..(rooms.len() / 2));
            let chosen_rooms = seq::index::sample(&mut rng, rooms.len(), room_idx)
                .into_iter()
                .map(|i| rooms[i].id)
                .collect();

            result.push((
                FractivityEntry {
                    id: Uuid::from_u128((result.len() - 1) as u128),
                    topic: format!("{}", result.len() - 1),
                    instructor_extension_uuids: chosen_instructors,
                    visitor_extension_uuids: chosen_visitors,
                    duration: generate_random_duration(),
                    allowed_rooms: chosen_rooms,
                    allowed_starts: generate_random_starts(i),
                    preparation_time: 5,
                    follow_up_time: 0,
                    blocker: false,
                },
                None,
            ));
        }
    }
    result
}

fn measure_single_distribution() -> (Duration, bool, f64) {
    let num_rooms = 22;
    let num_intructors = 20;
    let mut rng = rand::rng();

    let mut rooms: Vec<FractivityRoom> = Vec::new();
    for i in 0..num_rooms {
        rooms.push(FractivityRoom {
            id: Uuid::from_u128(i),
            priority: rng.random_range(-2..=2),
        });
    }

    let mut instructors: Vec<Uuid> = Vec::new();
    for i in 0..num_intructors {
        instructors.push(Uuid::from_u128(i));
    }

    let entries: Vec<(FractivityEntry, Option<(Uuid, i32)>)> =
        generate_random_fractivity_entries(&rooms, &instructors);

    use std::time::Instant;
    let now = Instant::now();

    let res = distribute(&entries, rooms.clone()).unwrap();

    let time = now.elapsed();

    let ref_now = Instant::now();

    let ref_res = checks::reference_distribution(&entries, rooms.clone()).unwrap();

    let ref_time = ref_now.elapsed();

    if res.is_err() {
        assert!(ref_res.is_err() || !check_distribution(&ref_res.unwrap()));
        assert!(!res.clone().unwrap_err().is_empty());
        let filtered_entries: Vec<(FractivityEntry, Option<(Uuid, i32)>)> = entries
            .clone()
            .into_iter()
            .filter(|(e, _)| !res.clone().unwrap_err().iter().any(|(f, _)| f.id == e.id))
            .collect();
        let filtered_res = distribute(&filtered_entries, rooms.clone()).unwrap();
        assert!(filtered_res.is_ok() && check_distribution(&filtered_res.unwrap()));
        let mapped_entries: Vec<(FractivityEntry, Option<(Uuid, i32)>)> = entries
            .clone()
            .iter_mut()
            .filter(|(e, _)| {
                !res.clone()
                    .unwrap_err()
                    .iter()
                    .any(|(f, r)| f.id == e.id && r == &RoomUnavailable)
            })
            .map(|(f, e)| {
                let binding = res.clone().unwrap_err();
                let bad_instructors = match binding.iter().find(|(f2, _)| f2.id == f.id) {
                    Some((_, FractivityDistributionFailReason::InstructorUnavailable(v))) => v,
                    Some((_, FractivityDistributionFailReason::RoomUnavailable)) => unreachable!(),
                    Some((_, FractivityDistributionFailReason::NoRoomsOrStarts)) => unreachable!(),
                    None => &Vec::new(),
                };
                f.instructor_extension_uuids = f
                    .instructor_extension_uuids
                    .clone()
                    .into_iter()
                    .filter(|i| !bad_instructors.contains(i))
                    .collect();
                (f.clone(), e.clone())
            })
            .collect();
        let mapped_res = distribute(&mapped_entries, rooms.clone()).unwrap();
        assert!(mapped_res.is_ok() && check_distribution(&mapped_res.unwrap()));
    } else {
        assert!(checks::check_distribution(&res.clone().unwrap()));
        //assert!(reference.is_ok());
    }

    (time, res.is_ok(), (ref_time.as_micros() as f64 / time.as_micros() as f64 - 1.0))
}

pub fn main() {
    let num_sucesses = 100000;
    let mut tries = 0;
    let mut total_duration = 0;
    let mut total_success_duration = 0;
    let mut total_successes = 0;
    let mut max_success = 0;
    let mut max_fail = 0;
    let mut total_improvement_factor = 0.0;
    let mut min_improvement = f64::MAX;
    while total_successes < num_sucesses {
        let dist_time = measure_single_distribution();
        total_duration += dist_time.0.as_millis();
        total_improvement_factor += dist_time.2;
        if dist_time.1 && dist_time.0.as_millis() > max_success {
            max_success = dist_time.0.as_millis();
        }
        if !dist_time.1 && dist_time.0.as_millis() > max_fail {
            max_fail = dist_time.0.as_millis();
        }
        if dist_time.2 < min_improvement {
            min_improvement = dist_time.2;
        }
        total_successes += if dist_time.1 { 1 } else { 0 };
        tries += 1;
        if dist_time.1 {
            total_success_duration += dist_time.0.as_millis();
        }
        std::fs::write("test.txt", format!("{:?}: Average execution: {:?}ms, average improvement: {:.2}%, min improvement: {:.2}% successes: {:?}({:?}%)\n successful:   average: {:?}ms,  max: {:?}ms \n unsuccessful: average: {:?}ms, max: {:?}ms",
                tries,
                total_duration / (tries ),
                total_improvement_factor / (tries as f64) * 100.0,
                min_improvement * 100.0,
                total_successes,
                (total_successes * 100) / (tries ),
                if total_successes > 0 {total_success_duration / total_successes} else {0},
                max_success,
                if (tries - total_successes) > 0 {(total_duration - total_success_duration) / (tries - total_successes)} else {0},
            max_fail)
            ).expect("Should be able to write to `/foo/tmp`");
    }
}
