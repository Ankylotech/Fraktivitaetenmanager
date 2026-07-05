use std::time::Duration;

use chrono::NaiveDate;
use rand::{seq, RngExt};
use rand_distr::{num_traits::ToPrimitive, Distribution, Normal};
use uuid::Uuid;
use Fraktivitaeten::{distribute, FractivityEntry, FractivityRoom};

#[cfg(test)]
pub mod tests {
    use chrono::NaiveDate;
    use uuid::Uuid;
    use Fraktivitaeten::{distribute, FractivityEntry, FractivityRoom};

    fn rooms() -> Vec<FractivityRoom> {
        [
            FractivityRoom {
                id: Uuid::from_u128(0),
                priority: 0,
            },
            FractivityRoom {
                id: uuid::Uuid::from_u128(1),
                priority: 0,
            },
            FractivityRoom {
                id: uuid::Uuid::from_u128(2),
                priority: 0,
            },
            FractivityRoom {
                id: uuid::Uuid::from_u128(3),
                priority: 0,
            },
        ]
        .to_vec()
    }

    fn undistributed_entries() -> Vec<(FractivityEntry, Option<(Uuid, i32)>)> {
        [
            (
                FractivityEntry {
                    id: uuid::Uuid::from_u128(0),
                    instructor_extension_uuids: vec![Uuid::from_u128(0)],
                    visitor_extension_uuids: Vec::new(),
                    duration: 30,
                    allowed_rooms: vec![Uuid::from_u128(0)],
                    allowed_starts: vec![0],
                    preparation_time: 0,
                    follow_up_time: 0,
                    start_day: NaiveDate::from_epoch_days(0).unwrap(),
                },
                None,
            ),
            (
                FractivityEntry {
                    id: uuid::Uuid::from_u128(1),
                    instructor_extension_uuids: vec![Uuid::from_u128(1)],
                    visitor_extension_uuids: Vec::new(),
                    duration: 30,
                    allowed_rooms: vec![Uuid::from_u128(0)],
                    allowed_starts: vec![0, 15, 30, 45],
                    preparation_time: 5,
                    follow_up_time: 0,
                    start_day: NaiveDate::from_epoch_days(0).unwrap(),
                },
                None,
            ),
            (
                FractivityEntry {
                    id: uuid::Uuid::from_u128(2),
                    instructor_extension_uuids: vec![Uuid::from_u128(0)],
                    visitor_extension_uuids: Vec::new(),
                    duration: 30,
                    allowed_rooms: vec![Uuid::from_u128(0), Uuid::from_u128(1)],
                    allowed_starts: vec![0, 15, 30, 45],
                    preparation_time: 5,
                    follow_up_time: 0,
                    start_day: NaiveDate::from_epoch_days(0).unwrap(),
                },
                None,
            ),
        ]
        .to_vec()
    }

    fn correct_distribution() -> Vec<(Uuid, i32)> {
        [
            (Uuid::from_u128(0), 0),
            (Uuid::from_u128(0), 45),
            (Uuid::from_u128(1), 45),
        ]
        .to_vec()
    }

    fn undistributable_entries() -> Vec<(FractivityEntry, Option<(Uuid, i32)>)> {
        [
            (
                FractivityEntry {
                    id: uuid::Uuid::from_u128(0),
                    instructor_extension_uuids: vec![Uuid::from_u128(0)],
                    visitor_extension_uuids: Vec::new(),
                    duration: 30,
                    allowed_rooms: vec![Uuid::from_u128(0)],
                    allowed_starts: vec![0],
                    preparation_time: 0,
                    follow_up_time: 0,
                    start_day: NaiveDate::from_epoch_days(0).unwrap(),
                },
                None,
            ),
            (
                FractivityEntry {
                    id: uuid::Uuid::from_u128(1),
                    instructor_extension_uuids: vec![Uuid::from_u128(1)],
                    visitor_extension_uuids: Vec::new(),
                    duration: 30,
                    allowed_rooms: vec![Uuid::from_u128(0)],
                    allowed_starts: vec![0, 15, 30, 45],
                    preparation_time: 5,
                    follow_up_time: 0,
                    start_day: NaiveDate::from_epoch_days(0).unwrap(),
                },
                None,
            ),
            (
                FractivityEntry {
                    id: uuid::Uuid::from_u128(2),
                    instructor_extension_uuids: vec![Uuid::from_u128(0), Uuid::from_u128(1)],
                    visitor_extension_uuids: Vec::new(),
                    duration: 30,
                    allowed_rooms: vec![Uuid::from_u128(1)],
                    allowed_starts: vec![0, 15, 30, 45],
                    preparation_time: 5,
                    follow_up_time: 0,
                    start_day: NaiveDate::from_epoch_days(0).unwrap(),
                },
                None,
            ),
            (
                FractivityEntry {
                    id: uuid::Uuid::from_u128(3),
                    instructor_extension_uuids: vec![Uuid::from_u128(2)],
                    visitor_extension_uuids: Vec::new(),
                    duration: 30,
                    allowed_rooms: vec![Uuid::from_u128(1)],
                    allowed_starts: vec![0, 15, 30, 45],
                    preparation_time: 5,
                    follow_up_time: 0,
                    start_day: NaiveDate::from_epoch_days(0).unwrap(),
                },
                None,
            ),
        ]
        .to_vec()
    }

    #[test]
    pub fn test_undistributed() {
        let entries = undistributed_entries();
        let result = distribute(&entries.clone(), &rooms());
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.len() == entries.len());
        let dist = correct_distribution();
        for i in 0..result.len() {
            let mut found = false;
            for j in 0..entries.len() {
                if result[i].0.id == entries[j].0.id {
                    found = true;
                    assert!(result[i].1 == dist[j].0 && result[i].2 == dist[j].1);
                }
            }
            assert!(found);
        }
    }

    #[test]
    pub fn test_undistributable() {
        let entries = undistributable_entries();
        let result = distribute(&mut entries.clone(), &rooms());
        assert!(result.is_err());
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
    v.to_i32().unwrap()
}

fn generate_random_starts(start: i32) -> Vec<i32> {
    let mut rng = rand::rng();
    let amount = rng.random_range(45..120);

    ((start)..(start + amount)).collect()
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
            let room_idx = rng.random_range(1..(rooms.len() / 2));
            let chosen_rooms = seq::index::sample(&mut rng, rooms.len(), room_idx)
                .into_iter()
                .map(|i| rooms[i].id)
                .collect();

            result.push((
                FractivityEntry {
                    id: Uuid::from_u128((result.len() - 1) as u128),
                    instructor_extension_uuids: chosen_instructors,
                    visitor_extension_uuids: Vec::new(),
                    duration: generate_random_duration(),
                    allowed_rooms: chosen_rooms,
                    allowed_starts: generate_random_starts(i),
                    preparation_time: 5,
                    follow_up_time: 0,
                    start_day: NaiveDate::from_epoch_days(0).unwrap(),
                },
                None,
            ));
        }
    }
    result
}

fn measure_single_distribution() -> (Duration, bool) {
    let num_rooms = 22;
    let num_intructors = 20;

    let mut rooms: Vec<FractivityRoom> = Vec::new();
    for i in 0..num_rooms {
        rooms.push(FractivityRoom {
            id: Uuid::from_u128(i),
            priority: 0,
        });
    }

    let mut instuctors: Vec<Uuid> = Vec::new();
    for i in 0..num_intructors {
        instuctors.push(Uuid::from_u128(i));
    }

    let entries: Vec<(FractivityEntry, Option<(Uuid, i32)>)> =
        generate_random_fractivity_entries(&rooms, &instuctors);

    use std::time::Instant;
    let now = Instant::now();

    let res = distribute(&entries, &rooms);

    (now.elapsed(), res.is_ok())
}

pub fn main() {
    let num_sucesses = 10;
    let mut tries = 0;
    let mut total_duration = 0;
    let mut total_success_duration = 0;
    let mut total_successes = 0;
    let mut max_duration = 0;
    while total_successes < num_sucesses {
        let dist_time = measure_single_distribution();
        total_duration += dist_time.0.as_millis();
        if dist_time.0.as_millis() > max_duration {
            max_duration = dist_time.0.as_millis();
        }
        total_successes += if dist_time.1 { 1 } else { 0 };
        tries += 1;
        if dist_time.1 {
            total_success_duration += dist_time.0.as_millis();
        }
        std::fs::write("test.txt", format!("{:?}: Average execution: {:?}ms, max distribution: {:?}ms with a total of {:?} successful distributions ({:?}%) taking {:?}ms on average",
                tries+1,
                total_duration / (tries + 1),
                max_duration,
                total_successes,
                (total_successes * 100) / (tries + 1),
                if total_successes > 0 {total_success_duration / total_successes} else {0})).expect("Should be able to write to `/foo/tmp`");
    }
}
