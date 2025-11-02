use chrono::NaiveDate;
use uuid::Uuid;

// Fields that should not matter are intentionaly removed
#[derive(
    Clone, Debug,
)]
pub struct FractivityRoom {
    pub id: Uuid,
    pub priority: i32,
}

#[derive(
    Clone, Debug,PartialEq, Eq
)]
pub struct FractivityEntry {
    pub id: Uuid,
    pub instructor_extension_uuids: Vec<Uuid>,
    pub duration: i32,
    pub allowed_rooms: Vec<Uuid>,
    pub allowed_starts: Vec<i32>,
    pub preparation_time: i32,
    pub follow_up_time: i32,
    pub start_day: NaiveDate,
}

pub fn distribute(
    values: &mut Vec<(FractivityEntry, Option<(Uuid, i32)>)>, all_rooms: &Vec<FractivityRoom>
) -> Result<Vec<(FractivityEntry, (Uuid, i32))>, FractivityEntry> {
    let mut distribute_cur = values.len();
    let mut max_len = -1;
    for i in 0..values.len() {
        if values[i].1.is_none() && values[i].0.allowed_starts.len() == 1 {
            distribute_cur = i;
            break;
        }
        if values[i].1.is_none() && values[i].0.duration > max_len {
            distribute_cur = i;
            max_len = values[i].0.duration;
        }
    }
    if distribute_cur == values.len() {
        return Ok(
            values
                .into_iter()
                .map(|(e, d)| match d {
                    Some(v) => (e.clone(), v.clone()),
                    None => unreachable!(),
                })
                .collect(),
        );
    }
    let mut last_end = 0;
    let mut starts = values[distribute_cur].0.allowed_starts.clone();
    let duration = values[distribute_cur].0.duration;
    for start in &starts {
        if *start + duration > last_end {
            last_end = *start + duration;
        }
    }
    let mut concurrent_fractivities = vec![0; (last_end as usize) + 1];
    for v in values.iter() {
        match v.1 {
            Some((_, start)) => {
                for i in 0..=v.0.duration {
                    if ((start + i) as usize) < concurrent_fractivities.len() {
                        concurrent_fractivities[(start + i) as usize] += 1;
                    } else {
                        break;
                    }
                }
            }
            None => (),
        }
    }
    let mut r: Vec<&FractivityRoom> = all_rooms.iter().filter(|r| values[distribute_cur].0.allowed_rooms.clone().contains(&r.id)).collect();
    r.sort_by_key(|f| -f.priority);
    let rooms: Vec<Uuid> = r.iter().map(|f| f.id).collect();
    let prep = values[distribute_cur].0.preparation_time;
    let follow = values[distribute_cur].0.follow_up_time;
    let day = values[distribute_cur].0.start_day;
    starts.sort_by_cached_key(|s| {
        concurrent_fractivities[(*s as usize)..=((*s + duration) as usize)]
            .iter()
            .max()
    });
    let mut result = Err(values[distribute_cur].0.clone());
    for start in starts {
        if result.is_ok() {
            break;
        }
        for room in &rooms {
            let mut legal = true;
            for v in values.iter() {
                if day != v.0.start_day {
                    continue;
                }
                match &v.1 {
                    Some((dist_id, dist_start)) => {
                        if (start - prep >= *dist_start - v.0.preparation_time
                            && start - prep < *dist_start + v.0.duration + v.0.follow_up_time)
                            || (*dist_start - v.0.preparation_time >= start - prep
                                && *dist_start - v.0.preparation_time < start + duration + follow)
                        {
                            if *dist_id == *room {
                                legal = false;
                                break;
                            }
                            legal = v.0.instructor_extension_uuids.iter().all(|id| {
                                !values[distribute_cur]
                                    .0
                                    .instructor_extension_uuids
                                    .contains(id)
                            });
                            if !legal {
                                break;
                            }
                        }
                    }
                    None => (),
                }
            }
            if !legal {
                continue;
            }
            let dist = (*room, start);
            values[distribute_cur].1 = Some(dist);
            result = distribute(values,all_rooms); 
            if result.is_ok() {
                return result;
            }
            values[distribute_cur].1 = None;
        }
    }
    result
}
