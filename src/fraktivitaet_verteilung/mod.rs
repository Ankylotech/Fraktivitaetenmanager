mod raum;
mod zeit;
mod personen;


use personen::Person;
use crate::fraktivitaetVerteilung::raum::Raum;
use crate::fraktivitaetVerteilung::zeit::{Zeitdauer, Zeitpunkt};

struct Fraktivitaet {
    id: usize,
    name: String,
    dauer: Zeitdauer,
    teilnehmer: Vec<Person>,
    zuteilung: Option<(Raum,Zeitpunkt)>,
    starts: Vec<Zeitpunkt>,
    raume: Vec<Raum>,
}