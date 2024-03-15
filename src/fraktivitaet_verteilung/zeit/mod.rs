use std::collections::HashMap;

#[derive(Debug, Copy, Clone, PartialOrd, PartialEq, Ord, Eq)]
pub struct Zeitdauer (usize);

impl Zeitdauer {
    pub fn add_dauer(&self, other: Zeitdauer) -> Zeitdauer {
        Zeitdauer(self.0 + other.0)
    }
}

#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct Zeitpunkt (usize);

impl Zeitpunkt {
    pub fn add_dauer(&self, d: &Zeitdauer) -> Zeitpunkt {
        Zeitpunkt(self.0 + d.0)
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Zeitraum(pub(crate) Zeitpunkt, Zeitdauer);

impl Zeitraum {

    pub fn new(punkt: Zeitpunkt, dauer: Zeitdauer) -> Zeitraum {
        Zeitraum(punkt, dauer)
    }

    pub fn new_punkte(start: Zeitpunkt, end: Zeitpunkt) -> Zeitraum {Zeitraum(start, Zeitdauer(end.0-start.0)) }

    pub fn inside(&self,punkt: Zeitpunkt) -> bool {
        punkt.0 >= self.0.0 && punkt.0 <= self.0.0 + self.1.0
    }

    pub fn overlapping(&self, other: &Zeitraum) -> bool {
        self.inside(other.0) || self.inside(other.0.add_dauer(&other.1)) || other.inside(self.0) || other.inside(self.0.add_dauer(&self.1))
    }

    pub fn add_dauer(&self, other: &Zeitdauer) -> Zeitraum {
        Zeitraum(self.0, self.1.add_dauer(*other))
    }
}

#[derive(Debug)]
pub struct Zeitmanager {
    start: String,
    stop: String,
    step: usize,
    excluded: Vec<Zeitraum>,
    punkt_array: Vec<String>,
    dauer_array: Vec<String>,
    punkt_map: HashMap<String,usize>,
    dauer_map: HashMap<String,usize>
}

impl Zeitmanager {
    pub fn new(start: &str, stop: &str, step: usize, exclude: Vec<(&str,&str)>) -> Self {
        let start = to_time(start);
        let stop = to_time(stop);
        let mut punkt_array = vec![start.clone()];
        let dauer_array = vec!["00:00".to_string()];
        let mut punkt_map = HashMap::new();
        let mut dauer_map = HashMap::new();
        let mut i = 0;
        let mut cur = start.clone();
        punkt_map.insert(cur.clone(),i);
        dauer_map.insert("00:00".to_string(),0);
        while !time_equal(&cur,&stop) {
            i += 1;
            cur = next_time(cur, step);
            punkt_map.insert(cur.clone(),i);
            punkt_array.push(cur.clone());
        }

        let mut current = Self {start,stop, step, punkt_array, punkt_map, dauer_array, dauer_map, excluded: Vec::new()};
        for (start, stop) in exclude {
            let start = current.string_to_punkt(start);
            let stop = current.string_to_punkt(stop);
            current.excluded.push(Zeitraum::new_punkte(start, stop));
        }
        current
    }

    pub fn string_to_punkt(&self, s: &str) -> Zeitpunkt {
        Zeitpunkt(*self.punkt_map.get(&to_time(s)).unwrap())
    }

    pub fn string_to_dauer(&mut self, time: &str) -> Zeitdauer {
        let time = to_time(time);
        let d = self.dauer_map.get(&time);
        match d {
            Some(v) => Zeitdauer(*v),
            None => {
                let mut i = self.dauer_array.len()-1;
                let mut cur = self.dauer_array[i].clone();
                while !time_equal(&time, &cur) {
                    i += 1;
                    cur = next_time(cur, self.step);
                    self.dauer_array.push(cur.clone());
                    self.dauer_map.insert(cur.clone(), i);
                }
                Zeitdauer(i)
            }
        }
    }

    pub fn punkt_to_string(&self, time: Zeitpunkt) -> String {
        self.punkt_array[time.0].clone()
    }

    pub fn dauer_to_string(&mut self, time: Zeitdauer) -> String {
        if time.0 < self.dauer_array.len() {
            self.dauer_array[time.0].clone()
        } else {
            let mut i = self.dauer_array.len() - 1;
            let mut cur = self.dauer_array[i].clone();
            while i < time.0 {
                i += 1;
                cur = next_time(cur, self.step);
                self.dauer_array.push(cur.clone());
                self.dauer_map.insert(cur.clone(), i);
            }
            cur.clone()
        }
    }

    pub fn zeitraum_to_string(&self, zeit: &Zeitraum) -> (String,String) {
        (self.punkt_to_string(zeit.0), self.punkt_to_string(zeit.0.add_dauer(&zeit.1)))
    }

    pub fn all_time(&self) -> Vec<Zeitpunkt> {
        let mut zeiten: Vec<Zeitpunkt> = self.punkt_map.values().map(|v| Zeitpunkt(*v)).collect();
        zeiten.sort_by_key(|z| z.0);
        zeiten
    }

    pub fn inside(&self, time: &Zeitraum) -> bool {
        let t = self.all_time();
        t.contains(&time.0) && t.contains(&time.0.add_dauer(&time.1)) && !self.excluded.iter().any(|z| z.overlapping(time))
    }
}

fn time_equal(time1: &String, time2: &String) -> bool {
    let split1: Vec<&str> = time1.split(":").collect();
    let split2: Vec<&str> = time2.split(":").collect();
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

fn to_time(time: &str) -> String {
    let split: Vec<&str> = time.split(":").collect();
    let mut hour = 0;
    let mut minute ;
    if split.len() == 1 {
        minute = split[0].parse::<usize>().unwrap();
    } else {
        hour = split[0].parse::<usize>().unwrap();
        minute = split[1].parse::<usize>().unwrap();
    }
    while minute >= 60 {
        hour += 1;
        minute -= 60;
        if hour == 24 {
            hour = 0;
        }
    }
    format!("{:0>2}:{:0>2}", hour, minute)
}

fn next_time(time: String, step: usize) -> String {
    let split: Vec<&str> = time.split(":").collect();
    let mut hour = 0;
    let mut minute ;
    if split.len() == 1 {
        minute = split[0].parse::<usize>().unwrap() + step;
    } else {
        hour = split[0].parse::<usize>().unwrap();
        minute = split[1].parse::<usize>().unwrap() + step;
    }
    while minute >= 60 {
        hour += 1;
        minute -= 60;
        if hour == 24 {
            hour = 0;
        }
    }
    format!("{:0>2}:{:0>2}", hour, minute)
}