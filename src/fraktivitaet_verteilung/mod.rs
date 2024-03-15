mod raum;
mod zeit;
mod personen;

use personen::Person;
use crate::fraktivitaet_verteilung::raum::Raum;
use crate::fraktivitaet_verteilung::zeit::{Zeitdauer, Zeitmanager, Zeitpunkt, Zeitraum};



#[derive(Debug, Clone)]
pub struct Fraktivitaet {
    name: String,
    dauer: Zeitdauer,
    teilnehmer: Vec<Person>,
    zuteilung: Option<(Raum,Zeitraum)>,
    starts: Vec<Zeitpunkt>,
    raume: Vec<Raum>,
    vorbereitung: Zeitdauer,
    nachbereitung: Zeitdauer,
}

impl Fraktivitaet {
    fn new(name: &str, dauer: Zeitdauer, teilnehmer: Vec<Person>, zuteilung: Option<(Raum,Zeitraum)>, starts: Vec<Zeitpunkt>, raume: Vec<Raum>, vorbereitung: Zeitdauer, nachbereitung: Zeitdauer) -> Self {
        let name = name.to_owned();
        Self {name, dauer, teilnehmer, zuteilung, starts, raume, vorbereitung, nachbereitung}
    }

    fn ueberschneidung(&self, other: &Self) -> bool {
        if self.zuteilung.is_none() || other.zuteilung.is_none() {
            return false;
        }
        let s = self.zuteilung.clone().unwrap();
        let o = other.zuteilung.clone().unwrap();
        let d1 = s.1.add_dauer(&self.nachbereitung).add_dauer(&other.vorbereitung);
        let d2 = o.1.add_dauer(&other.nachbereitung).add_dauer(&self.vorbereitung);

        return d1.overlapping(&d2) && (s.0.ueberschneidung(&o.0) || self.teilnehmer.iter().any(|p| other.teilnehmer.contains(p)))
    }
}

#[derive(Debug)]
pub struct FraktivitaetManager {
    personen: Vec<Person>,
    raume: Vec<Raum>,
    fraktivitaeten: Vec<Fraktivitaet>,
    manager: Zeitmanager,
}

impl FraktivitaetManager {
    fn add_fraktivitaet_all_info(&mut self, name: &str, dauer: &str, teilnehmer: Vec<&str>, raume: &Vec<&str>, ausgeschlossen: Vec<(&str,&str)>, zeit: Option<&str>, vorbereitung: Option<&str>, nachbereitung: Option<&str>) {
        let vorbereitung = vorbereitung.unwrap_or("0");
        let nachbereitung = nachbereitung.unwrap_or("0");
        let dauer = self.manager.string_to_dauer(dauer);
        let teilnehmer: Vec<Person> = teilnehmer.iter().map(|s| self.personen.iter().find(|p| p.is(s)).expect("Person not found").clone()).collect();
        let raume: Vec<Raum> = raume.iter().map(|s| self.raume.iter().find(|r| r.is(s)).expect("Raum not found").clone()).collect();
        if zeit.is_some() {
            let zeitraum = Zeitraum::new(self.manager.string_to_punkt(zeit.unwrap()),dauer);
            if raume.len() == 1 {
                self.fraktivitaeten.push(Fraktivitaet::new(name, dauer, teilnehmer, Some((raume[0].clone(),zeitraum)), Vec::new(), Vec::new(), self.manager.string_to_dauer(vorbereitung), self.manager.string_to_dauer(nachbereitung)));
                self.fraktivitaeten.sort_unstable_by_key(|f| f.dauer);
                self.fraktivitaeten.reverse();
                return
            } else {
                self.fraktivitaeten.push(Fraktivitaet::new(name,dauer,teilnehmer,None,vec![self.manager.string_to_punkt(zeit.unwrap())],raume, self.manager.string_to_dauer(vorbereitung), self.manager.string_to_dauer(nachbereitung)));
                self.fraktivitaeten.sort_unstable_by_key(|f| f.dauer);
                self.fraktivitaeten.reverse();
                return
            }
        }
        let ausgeschlossen: Vec<Zeitraum> = ausgeschlossen.iter().map(|(s1,s2)| Zeitraum::new_punkte(self.manager.string_to_punkt(s1), self.manager.string_to_punkt(s2))).collect();
        let mut zeiten = Vec::new();
        for k in self.manager.all_time() {
            let mut ausgeschl = false;
            let current = Zeitraum::new(k, dauer.clone());
            if !self.manager.inside(&current) {
                continue
            }
            for l in ausgeschlossen.clone() {
                if l.overlapping(&current) {
                    ausgeschl = true;
                    break;
                }
            }
            if !ausgeschl {
                zeiten.push(k);
            }
        }
        self.fraktivitaeten.push(Fraktivitaet::new(name,dauer,teilnehmer,None,zeiten,raume, self.manager.string_to_dauer(vorbereitung), self.manager.string_to_dauer(nachbereitung)));
        self.fraktivitaeten.sort_unstable_by_key(|f| f.dauer);
        self.fraktivitaeten.reverse();
    }

    pub fn add_fix_fraktivitaet(&mut self,name: &str, dauer: &str, teilnehmer: Vec<&str>, raum: &str, zeit: &str) {
        self.add_fraktivitaet_all_info(name, dauer, teilnehmer, &vec![raum], Vec::new(), Some(zeit), None, None);
    }

    pub fn add_fraktivitaet(&mut self,name: &str, dauer: &str, teilnehmer: Vec<&str>, raume: &Vec<&str>, ausgeschlossen: Vec<(&str,&str)>) {
        self.add_fraktivitaet_all_info(name,dauer,teilnehmer,raume,ausgeschlossen, None, None, None);
    }

    

    pub fn new(personen: Vec<&str>, raume: Vec<&str>) -> Self {
        let personen: Vec<Person> = personen.iter().map(|s| Person::new(s)).collect();
        let raume: Vec<Raum> = raume.iter().map(|r| Raum::new(r)).collect();

        let manager = Zeitmanager::new(super::START, super::END, super::STEP, super::EXCLUDED.to_vec());
        Self{personen,raume,manager, fraktivitaeten: Vec::new()}
    }

    pub fn legal(&self) -> bool {
        if self.fraktivitaeten.len() == 0 {
            return true;
        }
        for i in 0..(self.fraktivitaeten.len()-1) {
            for j in (i+1)..self.fraktivitaeten.len() {
                if self.fraktivitaeten[i].ueberschneidung(&self.fraktivitaeten[j]) {
                    return false
                }
            }
        }
        true
    }

    fn zuteilung_time_value(zeit1: Zeitraum, zeit2: &Zeitraum) -> usize {
        if zeit1.overlapping(&zeit2) {
            return if zeit1.0 == zeit2.0 {
                2
            } else {
                1
            }
        }
        return 0;
    }

    pub fn verteilen(&mut self) -> bool {
        let mut index = self.fraktivitaeten.len();
        for i in 0..self.fraktivitaeten.len() {
            if self.fraktivitaeten[i].zuteilung.is_none() {
                index = i;
                break;
            }
        }
        if index == self.fraktivitaeten.len() {
            return self.legal()
        }
        let fraks = self.fraktivitaeten.clone();
        let dauer = self.fraktivitaeten[index].dauer.clone();
        self.fraktivitaeten[index].starts.sort_by_key(|s| fraks.iter().map(|f| if f.zuteilung.is_some()  {Self::zuteilung_time_value(f.zuteilung.clone().unwrap().1, &Zeitraum::new(*s, dauer.clone()) )} else {0}).sum::<usize>());
        for zeit in 0..self.fraktivitaeten[index].starts.len() {
            let zeitraum = Zeitraum::new(self.fraktivitaeten[index].starts[zeit].clone(),self.fraktivitaeten[index].dauer);
            for raum in 0..self.fraktivitaeten[index].raume.len() {
                self.fraktivitaeten[index].zuteilung = Some((self.fraktivitaeten[index].raume[raum].clone(),zeitraum));
                if self.legal() && self.verteilen() {
                    return true
                }
                self.fraktivitaeten[index].zuteilung = None;
            }
        }

        false
    }

    pub fn print_verteilung(&self) {
        let mut fraks = self.fraktivitaeten.clone();
        fraks.sort_by_key(|f| f.name.clone());
        for frak in &fraks {
            if frak.zuteilung.is_some() {
                let (raum, zeit) = frak.zuteilung.clone().unwrap();
                let (punkt, ende) = self.manager.zeitraum_to_string(&zeit);
                println!("{}: {:?} ; {} - {}",frak.name, raum, punkt, ende);
            }
        }
    }

    pub fn json_verteilung(&self) -> String {
        let mut result = "[".to_string();
        let mut comma = false;
        let mut success = false;
        for frak in &self.fraktivitaeten {
            if frak.zuteilung.is_some() {
                let (raum, zeit) = frak.zuteilung.clone().unwrap();
                let (punkt, ende) = self.manager.zeitraum_to_string(&zeit);
                if comma {
                    result.push_str(",");
                }else {
                    comma = true;
                }
                result.push_str(format!("{{ \"name\": \"{}\" , \"raum\": \"{}\" , \"start\": \"{}\" , \"ende\": \"{}\"}} \n",frak.name, raum.get_name(), punkt, ende).as_str());
            }
        }
        result.push_str("]");
        return result;
    }

    pub fn json_teilnehmer(&self) -> String {
        let mut result = "[".to_string();
        let mut comma = false;
        for teilnehmer in &self.personen {
            if comma {
                result.push_str(",");
            }else {
                comma = true;
            }
            result.push_str(format!("{:?}", teilnehmer.get_name()).as_str())
        }
        result.push_str("]");
        return result;
    }

    pub fn json_raum(&self) -> String {
        let mut result = "[".to_string();
        let mut comma = false;
        for teilnehmer in &self.raume {
            if comma {
                result.push_str(",");
            }else {
                comma = true;
            }
            result.push_str(format!("{:?}", teilnehmer.get_name()).as_str())
        }
        result.push_str("]");
        return result;
    }

    pub fn is_person(&self, other: String) -> bool {
        self.personen.iter().any(|p| p.is(other.as_str()))
    }

    pub fn is_raum(&self, other: String) -> bool {
        self.raume.iter().any(|p| p.is(other.as_str()))
    }
}