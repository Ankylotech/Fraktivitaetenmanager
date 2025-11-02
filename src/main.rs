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

    fn distributed_entries() -> Vec<(FractivityEntry, Option<(Uuid, i32)>)> {
        [
            (
                FractivityEntry {
                    id: uuid::Uuid::from_u128(0),
                    instructor_extension_uuids: vec![Uuid::from_u128(0)],
                    duration: 60,
                    allowed_rooms: vec![Uuid::from_u128(0)],
                    allowed_starts: vec![0],
                    preparation_time: 0,
                    follow_up_time: 0,
                    start_day: NaiveDate::from_epoch_days(0).unwrap(),
                },
                Some((Uuid::from_u128(0), 0)),
            ),
            (
                FractivityEntry {
                    id: uuid::Uuid::from_u128(1),
                    instructor_extension_uuids: vec![Uuid::from_u128(1)],
                    duration: 60,
                    allowed_rooms: vec![Uuid::from_u128(1)],
                    allowed_starts: vec![0],
                    preparation_time: 0,
                    follow_up_time: 0,
                    start_day: NaiveDate::from_epoch_days(0).unwrap(),
                },
                Some((Uuid::from_u128(1), 0)),
            ),
            (
                FractivityEntry {
                    id: uuid::Uuid::from_u128(2),
                    instructor_extension_uuids: vec![Uuid::from_u128(1)],
                    duration: 60,
                    allowed_rooms: vec![Uuid::from_u128(1)],
                    allowed_starts: vec![60, 75, 90],
                    preparation_time: 0,
                    follow_up_time: 0,
                    start_day: NaiveDate::from_epoch_days(0).unwrap(),
                },
                Some((Uuid::from_u128(1), 75)),
            ),
        ]
        .to_vec()
    }

    fn undistributed_entries() -> Vec<(FractivityEntry, Option<(Uuid, i32)>)> {
        [
            (
                FractivityEntry {
                    id: uuid::Uuid::from_u128(0),
                    instructor_extension_uuids: vec![Uuid::from_u128(0)],
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
    pub fn test_distributed() {
        let entries = distributed_entries();
        let result = distribute(&mut entries.clone(), &rooms());
        assert!(result.is_ok());
        let result = result.unwrap();
        for i in 0..result.len() {
            assert!(result[i].0 == entries[i].0);
            assert!(result[i].1 == entries[i].1.unwrap());
        }
    }

    #[test]
    pub fn test_undistributed() {
        let entries = undistributed_entries();
        let result = distribute(&mut entries.clone(), &rooms());
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.len() == entries.len());
        let dist = correct_distribution();
        for i in 0..result.len() {
            let mut found = false;
            for j in 0..entries.len() {
                if result[i].0 == entries[j].0 {
                    found = true;
                    assert!(result[i].1 == dist[j]);
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
        let id = result.err().unwrap().id;
        assert!(id == entries[1].0.id || id == entries[2].0.id);
    }
}
