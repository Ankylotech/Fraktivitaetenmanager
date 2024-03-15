#![feature(ascii_char)]

mod fraktivitaet_verteilung;
use std::{
    fs,
    io::{prelude::*,BufReader},
    net::{TcpListener,TcpStream}
};
use Fraktivitaeten::ThreadPool;
use crate::fraktivitaet_verteilung::FraktivitaetManager;

const START: &str = "13:00";
const END: &str = "23:00";
const STEP: usize = 15;
const EXCLUDED: [(&str,&str); 1] = [("18:00","18:45")];
fn main() {
    let sport = vec!["Halle", "Fussballfeld", "Fussballfeld1", "Fussballfeld2"];
    let draussen = vec!["Fussballfeld", "Fussballfeld1", "Fussballfeld2"];
    let programmieren = vec!["Dachgarten"];
    let computer = vec!["Dachgarten"];
    let vortrag = vec!["Zusameck","Eichholz","Dachgarten"];
    let werken = vec!["Werkraum1", "Werkraum2", "Werkraum3"];
    let treffpunkt = vec!["Deck1", "Deck2"];
    let sonstiges = vec!["Sonstiges"];
    let mut raume = sport.clone();
    raume.append(&mut vortrag.clone());
    raume.append(&mut werken.clone());
    raume.append(&mut treffpunkt.clone());
    raume.append(&mut sonstiges.clone());

    let abend = ("19:00","22:45");
    let mittag = ("13:15","17:45");
    let nachmittag = ("16:15","17:45");
    let nach_mittagessen = ("13:15","13:15");
    let nach_kuchen = ("16:15","16:15");
    let nach_abendessen = ("19:00","19:00");
    let klein_nachtruhe = ("21:30","21:45");

    let personen = vec!["Louisa", "Tanja", "Felix", "Jonas K", "Jonas R", "Mika", "Nicht"];

    let mut frak_manager = fraktivitaet_verteilung::FraktivitaetManager::new(personen.clone(),raume);

    //frak_manager.add_fraktivitaet("Mathecamp Quiz", "1:00", vec!["Felix", "Nicht"], &vortrag, vec![mittag]);
    frak_manager.add_fraktivitaet("OSM", "1:00", vec!["Jonas K", "Nicht"], &treffpunkt, vec![abend]);
    frak_manager.add_fraktivitaet("Aluhut Zirkel", "1:00", vec!["Louisa", "Mika", "Nicht"], &vortrag, Vec::new());
    frak_manager.add_fraktivitaet("Improtheater", "1:00", vec!["Tanja", "Mika", "Nicht"], &vortrag, Vec::new());
    frak_manager.add_fraktivitaet("Git", "1:00", vec!["Mika"], &programmieren, Vec::new());
    frak_manager.add_fraktivitaet("Betreuer vs Kinder", "1:30", vec!["Felix", "Louisa", "Tanja", "Jonas R", "Jonas K"], &vec!["Halle"], vec![("13:00", "16:15")]);
    frak_manager.add_fix_fraktivitaet("Bunter Abend", "2:00", personen.clone(), "Zusameck", "21:00");

    //frak_manager.add_fraktivitaet("Programmieren", "2:00", vec!["Marc"], programmieren, Vec::new());
    /*frak_manager.add_fraktivitaet("Bildbearbeitung","2:00", vec!["Alex Mai"], &computer, vec![klein_nachtruhe]);
    frak_manager.add_fraktivitaet("Programmieren mit Emacs", "1:30", vec!["Marc"], &programmieren, Vec::new());
    frak_manager.add_fix_fraktivitaet("Orchester", "1:00", vec!["Pilex"], "aspachtal","13:15");
    frak_manager.add_fix_fraktivitaet("Matboj1", "2:15", vec!["Max", "Meru"], "himmelreich","13:15");
    frak_manager.add_fix_fraktivitaet("Matboj2", "1:30", vec!["Max"], "himmelreich","16:15");
    frak_manager.add_fraktivitaet("Malerei und Origami", "2:15", vec!["Mai"], &werken, vec![klein_nachtruhe]);
    frak_manager.add_fraktivitaet("Elektronische Bauteile", "2:15", vec!["Marc"], &werken, Vec::new());
    frak_manager.add_fraktivitaet("Wikingerschach bauen", "1:00", vec!["Ferdinand"], &werken, Vec::new());
    frak_manager.add_fraktivitaet("Völkerball", "1:00", vec!["Felix"], &vec!["fussballfeld"], vec![abend,nach_mittagessen,nach_kuchen]);
    frak_manager.add_fraktivitaet("Boomerang", "1:30", vec!["Samuel"], &sonstiges, vec![abend]);
    frak_manager.add_fraktivitaet("Slackline", "45", vec!["Kilian"], &sonstiges, vec![abend]);
    frak_manager.add_fix_fraktivitaet("Zirkel1", "1:30", vec!["Meru"], "zusameck","16:15");
    frak_manager.add_fix_fraktivitaet("Zirkel2", "1:30", vec!["Louisa"], "aspachtal", "16:15");
    frak_manager.add_fraktivitaet("Hilbertkalkül", "45", vec!["Marc"], &vortrag, Vec::new());
    frak_manager.add_fix_fraktivitaet("Chor", "1:00", vec!["Mai"], "aspachtal", "19:00");
    frak_manager.add_fraktivitaet("OSM", "1:00", vec!["Kilian"], &sonstiges, Vec::new());
    frak_manager.add_fraktivitaet("Capture the Flag", "1:00", vec!["Felix"], &vec!["fussballfeld"], vec![abend, nach_mittagessen, nach_kuchen]);
    frak_manager.add_fraktivitaet("Yoga", "45", vec!["Anna"], &vec!["halle"], Vec::new());
    frak_manager.add_fraktivitaet("Poi", "30", vec!["Alex Mai"], &sonstiges, Vec::new());
    frak_manager.add_fix_fraktivitaet("Gute Nacht Geschichte1", "30", vec!["Leonie"], "buecherstube", "21:00");
    frak_manager.add_fix_fraktivitaet("Gute Nacht Geschichte2", "30", vec!["Marc"], "rehgraben", "21:30");*/

    if !frak_manager.verteilen() {
        println!("Impossible");
        return;
    };
    frak_manager.print_verteilung();

    let listener = TcpListener::bind("127.0.0.1:8080").unwrap();

    for stream in listener.incoming() {
        let stream = stream.unwrap();
        handle_connection(stream, &mut frak_manager)
    }

}

fn handle_connection(mut stream: TcpStream, frak_manager: &mut FraktivitaetManager){
    let buf_reader = BufReader::new(&mut stream);
    let mut l1 = false;
    let request: Vec<_> = buf_reader.lines().map( |line| line.unwrap()).take_while(|line| {
        if !line.is_empty() {
            if line.contains("GET") {
                l1 = true;
            }
            true
        } else if !l1 {
            l1 = true;
            true
        } else {
            false
        }
    }).collect();
    println!("{:?}", request);
    let (status,response) = match request[0].as_str() {
        "GET / HTTP/1.1" => {
            ("HTTP/1.1 200 OK",Response::HTML("main/index.html".to_string()))
        },
        "GET /verteilung HTTP/1.1" => {
            ("text/json 200 OK",Response::FRAKTIVITAETEN)
        },
        "GET /teilnehmer HTTP/1.1" => {
            ("text/json 200 OK",Response::TEILNEHMER)
        },
        "GET /raum HTTP/1.1" => {
            ("text/json 200 OK",Response::RAUM)
        },
        request => {
            let split: Vec<_> = request.split(" ").collect();
            if split.len() >= 2 && split[1].starts_with("/verteilung") {
                let payload = split[1].split("?").collect::<Vec<&str>>()[1];
                let data = payload.split("&");
                let mut name = "";
                let mut dauer = "";
                let mut teilnehmer: Vec<&str> = Vec::new();
                let mut raum: Vec<&str> = Vec::new();
                for s in data {
                    let v = s.split("=").collect::<Vec<&str>>();
                    if v[0] == "FraktivitaetName" {
                        name = v[1];
                    } else if v[0] == "FraktivitaetZeit" {
                        dauer = v[1];
                    } else if v[1] == "on" && frak_manager.is_person(v[0].to_owned()) {
                        teilnehmer.push(v[0]);
                    } else if v[1] == "on" && frak_manager.is_raum(v[0].to_owned()) {
                        raum.push(v[0]);
                    }
                }
                frak_manager.add_fraktivitaet(name, dauer, teilnehmer, &raum, Vec::new());
                frak_manager.verteilen();
                ("HTTP/1.1 200 OK",Response::HTML("main/index.html".to_string()))
            } else {
                ("HTTP/1.1 404 Not Found", Response::HTML("404/404.html".to_string()))
            }
        }
    };
    let response = match response {
        Response::HTML(filename) => {
            let content = fs::read_to_string(filename).unwrap();
            let length = content.len();
            format!("{status}\r\nContent-Length: {length}\r\n\r\n{content}")
        },
        Response::FRAKTIVITAETEN => frak_manager.json_verteilung(),
        Response::TEILNEHMER => frak_manager.json_teilnehmer(),
        Response::RAUM => frak_manager.json_raum(),
    };
    println!("Writing response");
    stream.write_all(response.as_bytes()).unwrap();
}

enum Response {
    HTML(String),
    FRAKTIVITAETEN,
    TEILNEHMER,
    RAUM
}