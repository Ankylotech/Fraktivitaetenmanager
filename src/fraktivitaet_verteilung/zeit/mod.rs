
pub struct Zeitdauer (usize);

pub struct Zeitpunkt (usize);

pub struct Zeitmanager {
    start: String,
    step: usize,
}

impl Zeitmanager {
    pub fn new(start: String, step: usize) -> Self {
        Self {start, step}
    }

    pub fn string_to_punkt(&self, s: String) -> Zeitpunkt {
        let mut i = 0;
        let mut cur = self.start.clone();
        while !time_equal(&s,&cur) {
            i += 1;
            cur = next_time(cur, self.step);
        }
        Zeitpunkt(i)
    }
}

fn time_equal(time1: &String, time2: &String) -> bool {
    let split1: Vec<String> = time1.split(":").collect();
    let split2: Vec<String> = time2.split(":").collect();
    return if split1.len() == 1 {
        if split2.len() == 1 {
            split1[0].parse::<usize>().unwrap() == split2[0].parse::<usize>().unwrap()
        } else {
            split1[0].parse::<usize>().unwrap() == split2[1].parse::<usize>().unwrap() && split2[0].parse::<usize>().unwrap() == 0
        }
    } else {
        if split2.len() == 1 {
            split1[1].parse::<usize>().unwrap() == split2[0].parse::<usize>().unwrap() && split1[0].parse::<usize>().unwrap() == 0
        } else {
            split1[0].parse::<usize>().unwrap() == split2[0].parse::<usize>().unwrap() &&
                split1[1].parse::<usize>().unwrap() == split2[1].parse::<usize>().unwrap()
        }
    }
}

fn next_time(time: String, step: usize) -> String {
    let split: Vec<String> = time.split(":").collect();
    if split.len() == 1 {
        let s: String = (split[0].parse::<usize>().unwrap() + step).to_string();
        s
    } else {
        let mut hour = split[0].parse::<usize>().unwrap();
        let mut minutes = split[1].parse::<usize>().unwrap();
        minutes += step;
        while minutes > 60 {
            hour += 1;
            minutes -= 60;
            if hour == 24 {
                hour = 0;
            }
        }
        format!("{}:{}", hour, minutes)
    }
}